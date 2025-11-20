#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use dioxus::prelude::*;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    // Configurar el título de la ventana
    #[cfg(not(target_arch = "wasm32"))]
    {
        use dioxus::desktop::use_window;
        let window = use_window();
        window.set_title("DXF to ELMT Converter");
    }
    
    let mut selected_path = use_signal(|| Option::<String>::None);
    let mut last_output_dir = use_signal(|| Option::<String>::None);
    let mut spline_step = use_signal(|| 20u32);
    let mut px_per_mm_px = use_signal(|| 2.0f64);
    let mut px_per_mm_mm = use_signal(|| 1.0f64);
    let mut verbose = use_signal(|| false);
    let mut info_flag = use_signal(|| false);
    let mut is_processing = use_signal(|| false);
    let mut status = use_signal(|| String::new());
    let mut preview_stats = use_signal(|| Option::<dxf2elmt::ConversionStats>::None);

    rsx! {
        div {
            style: "max-width: 850px; min-height: 100vh; margin: 0 auto; padding: 20px; font-family: system-ui, -apple-system, sans-serif; box-sizing: border-box;",
            h1 { style: "color: #2563eb; margin-bottom: 10px;", "DXF to ELMT Converter" }

            div {
                style: "background: #f9fafb; border: 1px solid #e5e7eb; border-radius: 8px; padding: 16px; margin-top: 12px; display: flex; flex-direction: column; gap: 12px;",
                button {
                    style: "background: #2563eb; color: white; border: none; padding: 10px 16px; border-radius: 6px; cursor: pointer; width: fit-content;",
                    onclick: move |_| {
                        if is_processing() { return; }
                        let file = rfd::FileDialog::new()
                            .add_filter("DXF", &["dxf"])
                            .set_title("Selecciona un archivo DXF")
                            .pick_file();
                        if let Some(path) = file {
                            selected_path.set(Some(path.display().to_string()));
                            status.set(String::new());
                            preview_stats.set(None);
                            // Cargar y mostrar resumen de entidades (previo a convertir)
                            let path_for_preview = path.clone();
                            dioxus::core::spawn(async move {
                                use dxf::Drawing;
                                use dxf::entities::EntityType;
                                let res = std::thread::spawn(move || {
                                    let drawing = Drawing::load_file(&path_for_preview).map_err(|e| e.to_string())?;
                                    let mut circles = 0u32;
                                    let mut lines = 0u32;
                                    let mut arcs = 0u32;
                                    let mut splines = 0u32;
                                    let mut texts = 0u32;
                                    let mut ellipses = 0u32;
                                    let mut polylines = 0u32;
                                    let mut lwpolylines = 0u32;
                                    let mut solids = 0u32;
                                    let mut blocks = 0u32;
                                    let mut unsupported = 0u32;
                                    drawing.entities().for_each(|e| match e.specific {
                                        EntityType::Circle(_) => circles += 1,
                                        EntityType::Line(_) => lines += 1,
                                        EntityType::Arc(_) => arcs += 1,
                                        EntityType::Spline(_) => splines += 1,
                                        EntityType::Text(_) => texts += 1,
                                        EntityType::Ellipse(_) => ellipses += 1,
                                        EntityType::Polyline(_) => polylines += 1,
                                        EntityType::LwPolyline(_) => lwpolylines += 1,
                                        EntityType::Solid(_) => solids += 1,
                                        EntityType::Insert(_) => blocks += 1,
                                        _ => unsupported += 1,
                                    });
                                    Ok::<_, String>(dxf2elmt::ConversionStats {
                                        circles, lines, arcs, splines, texts, ellipses,
                                        polylines, lwpolylines, solids, blocks, unsupported,
                                        elapsed_ms: 0,
                                    })
                                }).join();
                                match res {
                                    Ok(Ok(stats)) => preview_stats.set(Some(stats)),
                                    Ok(Err(e)) => status.set(format!("Error leyendo DXF: {e}")),
                                    Err(_) => status.set("Error: fallo interno leyendo DXF".to_string()),
                                }
                            });
                        }
                    },
                    "Seleccionar DXF..."
                }
                if let Some(path) = selected_path() {
                    div { "Seleccionado: {path}" }
                }
                if let Some(st) = preview_stats() {
                    div {
                        style: "background: #eef2ff; border: 1px solid #c7d2fe; border-radius: 8px; padding: 12px;",
                        h3 { style: "margin: 0 0 8px 0; color: #1e3a8a;", "Resumen de entidades" }
                        ul {
                            li { "Circles: {st.circles}" }
                            li { "Lines: {st.lines}" }
                            li { "Arcs: {st.arcs}" }
                            li { "Splines: {st.splines}" }
                            li { "Texts: {st.texts}" }
                            li { "Ellipses: {st.ellipses}" }
                            li { "Polylines: {st.polylines}" }
                            li { "LwPolylines: {st.lwpolylines}" }
                            li { "Solids: {st.solids}" }
                            li { "Blocks: {st.blocks}" }
                            li { "Unsupported: {st.unsupported}" }
                        }
                    }
                }

                div {
                    style: "display: flex; flex-direction: column; gap: 12px;",
                    div {
                        style: "display: flex; align-items: center; gap: 8px; flex-wrap: nowrap;",
                        label { style: "white-space: nowrap;", "Spline step:" }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "200",
                            value: "{spline_step()}",
                            title: "Indica cuántos puntos usar para aproximar una spline. Un valor mayor mejora la precisión, pero aumenta el tamaño del archivo y el tiempo de procesamiento. Valor por defecto: 20",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<u32>() { spline_step.set(v); }
                            },
                            style: "width: 80px; padding: 6px; border: 1px solid #d1d5db; border-radius: 4px;"
                        }
                        label { style: "white-space: nowrap;", "Relación:" }
                        input {
                            r#type: "number",
                            min: "0.1",
                            max: "100",
                            step: "0.1",
                            value: "{px_per_mm_px()}",
                            title: "Número de píxeles en la relación px/mm",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<f64>() {
                                    if v >= 0.1 && v <= 100.0 {
                                        px_per_mm_px.set(v);
                                    }
                                }
                            },
                            style: "width: 60px; padding: 6px; border: 1px solid #d1d5db; border-radius: 4px;"
                        }
                        span { style: "white-space: nowrap;", "px /" }
                        input {
                            r#type: "number",
                            min: "0.1",
                            max: "100",
                            step: "0.1",
                            value: "{px_per_mm_mm()}",
                            title: "Número de milímetros en la relación px/mm",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<f64>() {
                                    if v >= 0.1 && v <= 100.0 && v > 0.0 {
                                        px_per_mm_mm.set(v);
                                    }
                                }
                            },
                            style: "width: 60px; padding: 6px; border: 1px solid #d1d5db; border-radius: 4px;"
                        }
                        span { style: "white-space: nowrap;", "mm" }
                    }
                    div {
                        style: "display: flex; align-items: center; gap: 12px;",
                        label {
                            input {
                                r#type: "checkbox",
                                checked: verbose(),
                                oninput: move |e| verbose.set(e.value() == "on")
                            }
                            span { " verbose (imprime XML en vez de escribir archivo)" }
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: info_flag(),
                                oninput: move |e| info_flag.set(e.value() == "on")
                            }
                            span { " info (estadísticas)" }
                        }
                    }
                }

                button {
                    disabled: is_processing() || selected_path().is_none(),
                    style: "background: #16a34a; color: white; border: none; padding: 10px 16px; border-radius: 6px; cursor: pointer; width: fit-content;",
                    onclick: move |_| {
                        if is_processing() { return; }
                        if let Some(path_str) = selected_path() {
                            is_processing.set(true);
                            status.set("Convirtiendo...".to_string());
                            let path_owned = path_str.clone();
                            let v = verbose();
                            let i = info_flag();
                            let step = spline_step();
                            let px = px_per_mm_px();
                            let mm = px_per_mm_mm();
                            let px_per_mm = px / mm;
                            dioxus::core::spawn(async move {
                                use dxf2elmt::{convert_dxf_file, ConversionOptions};
                                use std::path::PathBuf;
                                use std::path::Path;
                                let result = std::thread::spawn(move || {
                                    let pb = PathBuf::from(path_owned);
                                    let opts = ConversionOptions { spline_step: step, verbose: v, info: i, px_per_mm };
                                    convert_dxf_file(&pb, &opts)
                                }).join();
                                match result {
                                    Ok(Ok(conv)) => {
                                        // Guardamos la carpeta de salida si no es verbose (se escribe archivo)
                                        if !v {
                                            if let Some(parent) = Path::new(&selected_path().unwrap_or_default()).parent() {
                                                last_output_dir.set(Some(parent.display().to_string()));
                                            }
                                        }
                                        status.set(format!("OK: {}", conv.message));
                                    }
                                    Ok(Err(e)) => status.set(format!("Error: {e}")),
                                    Err(_) => status.set("Error: fallo interno al convertir".to_string()),
                                }
                                is_processing.set(false);
                            });
                        }
                    },
                    if is_processing() { "Convirtiendo..." } else { "Convertir a ELMT" }
                }
                if !status().is_empty() {
                    div { style: "color: #111827;", "{status()}" }
                }
                button {
                    disabled: last_output_dir().is_none(),
                    style: "background: #374151; color: white; border: none; padding: 10px 16px; border-radius: 6px; cursor: pointer; width: fit-content;",
                    onclick: move |_| {
                        if let Some(dir) = last_output_dir() {
                            #[cfg(target_os = "windows")]
                            {
                                let _ = std::process::Command::new("explorer").arg(dir).spawn();
                            }
                            #[cfg(target_os = "linux")]
                            {
                                let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                            }
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("open").arg(dir).spawn();
                            }
                        }
                    },
                    "Abrir carpeta"
                }
            }
        }
    }
}
