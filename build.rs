#[cfg(windows)]
fn main() {
    // Solo configurar el icono para el binario desktop
    if std::env::var("CARGO_BIN_NAME").unwrap_or_default() == "dxf2elmt-desktop" {
        // Verificar si existe el archivo de icono
        if std::path::Path::new("assets/icon.ico").exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            if let Err(e) = res.compile() {
                eprintln!("Warning: No se pudo compilar el icono: {}", e);
            }
        } else {
            println!("cargo:warning=No se encontró assets/icon.ico. El ejecutable usará el icono por defecto.");
        }
    }
}

#[cfg(not(windows))]
fn main() {
    // No hacer nada en sistemas no Windows
}

