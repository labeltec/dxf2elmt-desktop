#![warn(
    clippy::all,
    clippy::pedantic,
)]

pub mod qelmt;
pub mod file_writer;

use anyhow::{Context, Result};
use dxf::entities::EntityType;
use dxf::Drawing;
use qelmt::{Definition, Objects};
use simple_xml_builder::XMLElement;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversionStats {
    pub circles: u32,
    pub lines: u32,
    pub arcs: u32,
    pub splines: u32,
    pub texts: u32,
    pub ellipses: u32,
    pub polylines: u32,
    pub lwpolylines: u32,
    pub solids: u32,
    pub blocks: u32,
    pub unsupported: u32,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversionResult {
    pub success: bool,
    pub message: String,
    pub stats: Option<ConversionStats>,
    pub xml_content: Option<String>,
}

pub struct ConversionOptions {
    pub spline_step: u32,
    pub verbose: bool,
    pub info: bool,
    pub px_per_mm: f64, // Relación píxeles por milímetro (por defecto: 2.0 px/mm)
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            spline_step: 20,
            verbose: false,
            info: false,
            px_per_mm: 2.0, // Por defecto: 2px / 1mm
        }
    }
}

pub fn convert_dxf_file(
    file_path: &Path,
    options: &ConversionOptions,
) -> Result<ConversionResult> {
    let now = Instant::now();
    let friendly_file_name = file_path
        .file_stem()
        .unwrap_or_else(|| file_path.as_os_str())
        .to_string_lossy()
        .to_string();

    // Load DXF file
    let drawing = Drawing::load_file(file_path).context(format!(
        "Failed to load {friendly_file_name}...\n\tMake sure the file is a valid .dxf file.",
    ))?;

    let q_elmt = Definition::new(friendly_file_name.clone(), options.spline_step, options.px_per_mm, &drawing);

    // Initialize counts
    let mut circle_count: u32 = 0;
    let mut line_count: u32 = 0;
    let mut arc_count: u32 = 0;
    let mut spline_count: u32 = 0;
    let mut text_count: u32 = 0;
    let mut ellipse_count: u32 = 0;
    let mut polyline_count: u32 = 0;
    let mut lwpolyline_count: u32 = 0;
    let mut solid_count: u32 = 0;
    let mut block_count: u32 = 0;
    let mut other_count: u32 = 0;

    // Loop through all entities, counting the element types
    drawing.entities().for_each(|e| match e.specific {
        EntityType::Circle(_) => circle_count += 1,
        EntityType::Line(_) => line_count += 1,
        EntityType::Arc(_) => arc_count += 1,
        EntityType::Spline(_) => spline_count += 1,
        EntityType::Text(_) => text_count += 1,
        EntityType::Ellipse(_) => ellipse_count += 1,
        EntityType::Polyline(_) => polyline_count += 1,
        EntityType::LwPolyline(_) => lwpolyline_count += 1,
        EntityType::Solid(_) => solid_count += 1,
        EntityType::Insert(_) => block_count += 1,
        _ => other_count += 1,
    });

    // Generate XML
    let out_xml = XMLElement::from(&q_elmt);
    let xml_content = if options.verbose {
        Some(format!("{}", out_xml))
    } else {
        None
    };

    let elapsed_ms = now.elapsed().as_millis();
    let stats = ConversionStats {
        circles: circle_count,
        lines: line_count,
        arcs: arc_count,
        splines: spline_count,
        texts: text_count,
        ellipses: ellipse_count,
        polylines: polyline_count,
        lwpolylines: lwpolyline_count,
        solids: solid_count,
        blocks: block_count,
        unsupported: other_count,
        elapsed_ms,
    };

    // Create output file if not verbose
    if !options.verbose {
        let out_file = file_writer::create_file(false, options.info, file_path)?;
        out_xml
            .write(&out_file)
            .context("Failed to write output file.")?;
        
        // Crear archivo de log con información de textos convertidos
        write_text_log(file_path, &q_elmt.description, &stats)?;
    }

    Ok(ConversionResult {
        success: true,
        message: format!("Successfully converted {}", friendly_file_name),
        stats: Some(stats),
        xml_content,
    })
}

// Función para escribir el archivo de log con información de textos convertidos
fn write_text_log(file_path: &Path, description: &qelmt::Description, stats: &ConversionStats) -> Result<()> {
    // Crear ruta del archivo de log
    let log_path = file_path.with_extension("log");
    let mut log_file = File::create(&log_path)
        .context(format!("Failed to create log file: {}", log_path.display()))?;
    
    // Contar entidades convertidas en ELMT
    let mut elmt_circles = 0u32;
    let mut elmt_lines = 0u32;
    let mut elmt_arcs = 0u32;
    let mut elmt_polygons = 0u32;
    let mut elmt_dynamic_texts = 0u32;
    let mut elmt_texts = 0u32;
    let mut elmt_groups = 0u32;
    
    fn count_objects_recursive(objects: &[qelmt::Objects], 
        circles: &mut u32, lines: &mut u32, arcs: &mut u32, 
        polygons: &mut u32, dynamic_texts: &mut u32, texts: &mut u32, groups: &mut u32) {
        for obj in objects {
            match obj {
                qelmt::Objects::Arc(_) => *arcs += 1,
                qelmt::Objects::Ellipse(_) => *circles += 1, // Los círculos se convierten en elipses
                qelmt::Objects::Polygon(_) => *polygons += 1,
                qelmt::Objects::DynamicText(_) => *dynamic_texts += 1,
                qelmt::Objects::Text(_) => *texts += 1,
                qelmt::Objects::Line(_) => *lines += 1,
                qelmt::Objects::Group(group_objects) => {
                    *groups += 1;
                    count_objects_recursive(group_objects, circles, lines, arcs, polygons, dynamic_texts, texts, groups);
                }
            }
        }
    }
    
    count_objects_recursive(&description.objects, &mut elmt_circles, &mut elmt_lines, 
        &mut elmt_arcs, &mut elmt_polygons, &mut elmt_dynamic_texts, &mut elmt_texts, &mut elmt_groups);
    
    // Escribir encabezado
    writeln!(log_file, "=== Log de conversión DXF a ELMT ===")?;
    writeln!(log_file, "Archivo: {}", file_path.display())?;
    writeln!(log_file, "Tiempo de procesamiento: {} ms\n", stats.elapsed_ms)?;
    
    // Estadísticas de entidades en el DXF
    writeln!(log_file, "=== ESTADÍSTICAS DE ENTIDADES EN EL ARCHIVO DXF ===")?;
    writeln!(log_file, "Círculos: {}", stats.circles)?;
    writeln!(log_file, "Líneas: {}", stats.lines)?;
    writeln!(log_file, "Arcos: {}", stats.arcs)?;
    writeln!(log_file, "Splines: {}", stats.splines)?;
    writeln!(log_file, "Textos: {}", stats.texts)?;
    writeln!(log_file, "Elipses: {}", stats.ellipses)?;
    writeln!(log_file, "Polylines: {}", stats.polylines)?;
    writeln!(log_file, "LwPolylines: {}", stats.lwpolylines)?;
    writeln!(log_file, "Sólidos: {}", stats.solids)?;
    writeln!(log_file, "Bloques: {}", stats.blocks)?;
    writeln!(log_file, "Entidades no soportadas: {}", stats.unsupported)?;
    writeln!(log_file, "Total: {}\n", 
        stats.circles + stats.lines + stats.arcs + stats.splines + stats.texts + 
        stats.ellipses + stats.polylines + stats.lwpolylines + stats.solids + 
        stats.blocks + stats.unsupported)?;
    
    // Estadísticas de entidades convertidas en ELMT
    writeln!(log_file, "=== ESTADÍSTICAS DE ENTIDADES CONVERTIDAS EN ELMT ===")?;
    writeln!(log_file, "Círculos/Elipses: {}", elmt_circles)?;
    writeln!(log_file, "Líneas: {}", elmt_lines)?;
    writeln!(log_file, "Arcos: {}", elmt_arcs)?;
    writeln!(log_file, "Polígonos (incluye Splines y Polylines): {}", elmt_polygons)?;
    writeln!(log_file, "Textos dinámicos: {}", elmt_dynamic_texts)?;
    writeln!(log_file, "Textos estáticos: {}", elmt_texts)?;
    writeln!(log_file, "Grupos (Bloques): {}", elmt_groups)?;
    writeln!(log_file, "Total: {}\n", 
        elmt_circles + elmt_lines + elmt_arcs + elmt_polygons + 
        elmt_dynamic_texts + elmt_texts + elmt_groups)?;
    
    // Entidades no convertidas
    if stats.unsupported > 0 {
        writeln!(log_file, "=== ADVERTENCIA: ENTIDADES NO CONVERTIDAS ===")?;
        writeln!(log_file, "Se encontraron {} entidades que no pudieron ser convertidas.\n", stats.unsupported)?;
    }
    
    writeln!(log_file, "=== DETALLE DE TEXTOS CONVERTIDOS ===\n")?;
    
    // Contador de textos
    let mut text_index = 1;
    
    // Función recursiva para procesar objetos (incluyendo grupos)
    fn process_objects(
        objects: &[Objects],
        log_file: &mut File,
        text_index: &mut usize,
    ) -> Result<()> {
        for obj in objects {
            match obj {
                Objects::DynamicText(dtext) => {
                    writeln!(log_file, "--- Texto {} (DynamicText) ---", text_index)?;
                    writeln!(log_file, "Contenido: \"{}\"", dtext.text.replace('\n', "\\n"))?;
                    writeln!(log_file, "Posición: x={:.2}, y={:.2}, z={:.2}", dtext.x, dtext.y, dtext.z)?;
                    writeln!(log_file, "Rotación: {:.2}°", dtext.rotation)?;
                    writeln!(log_file, "Tamaño de entrada (DXF): {:.2} unidades DXF", dtext.original_text_height)?;
                    writeln!(log_file, "Tamaño de salida (ELMT): {:.2}pt", dtext.font.point_size)?;
                    if dtext.original_text_height > 0.0 {
                        writeln!(log_file, "Factor de escala texto aplicado: {:.2}", dtext.font.point_size / dtext.original_text_height)?;
                    }
                    writeln!(log_file, "Fuente: familia=\"{}\"", dtext.font.family)?;
                    writeln!(log_file, "Estilo: weight={}, style={:?}", dtext.font.weight, dtext.font.style)?;
                    writeln!(log_file, "Color: {}", dtext.color.display_rgb())?;
                    writeln!(log_file, "Alineación: H={:?}, V={:?}", dtext.h_alignment, dtext.v_alignment)?;
                    writeln!(log_file, "Ancho de referencia: {:.2}", dtext.reference_rectangle_width)?;
                    writeln!(log_file, "Frame: {}", dtext.frame)?;
                    writeln!(log_file, "UUID: {}", dtext.uuid)?;
                    if let Some(ref info_name) = dtext.info_name {
                        writeln!(log_file, "Info name: {}", info_name)?;
                    }
                    writeln!(log_file, "")?;
                    *text_index += 1;
                }
                Objects::Text(text) => {
                    writeln!(log_file, "--- Texto {} (Text) ---", text_index)?;
                    writeln!(log_file, "Contenido: \"{}\"", text.value)?;
                    writeln!(log_file, "Posición: x={:.2}, y={:.2}", text.x, text.y)?;
                    writeln!(log_file, "Rotación: {:.2}°", text.rotation)?;
                    writeln!(log_file, "Tamaño de entrada (DXF): {:.2} unidades DXF", text.original_text_height)?;
                    writeln!(log_file, "Tamaño de salida (ELMT): {:.2}pt", text.font.point_size)?;
                    if text.original_text_height > 0.0 {
                        writeln!(log_file, "Factor de escala texto aplicado: {:.2}", text.font.point_size / text.original_text_height)?;
                    }
                    writeln!(log_file, "Fuente: familia=\"{}\"", text.font.family)?;
                    writeln!(log_file, "Estilo: weight={}, style={:?}", text.font.weight, text.font.style)?;
                    writeln!(log_file, "Color: {}", text.color.display_rgb())?;
                    writeln!(log_file, "")?;
                    *text_index += 1;
                }
                Objects::Group(group_objects) => {
                    // Procesar recursivamente los objetos del grupo
                    process_objects(group_objects, log_file, text_index)?;
                }
                _ => {} // Ignorar otros tipos de objetos
            }
        }
        Ok(())
    }
    
    // Procesar todos los objetos
    process_objects(&description.objects, &mut log_file, &mut text_index)?;
    
    writeln!(log_file, "=== Fin del log ===")?;
    writeln!(log_file, "Total de textos convertidos: {}", text_index - 1)?;
    
    Ok(())
}

