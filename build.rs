//! Junta os mapeamentos de controladora (`mapeamentos/*.djosmap`) e as skins (`skins/*.djosskin`)
//! dentro do programa (o AppImage, o .exe e o aparelho já saem com todos, sem arquivo solto).

fn main() {
    juntar("mapeamentos", "djosmap", "mapeamentos.rs");
    juntar("skins", "djosskin", "skins.rs");
}

/// Escreve no OUT_DIR uma lista `&[("arquivo", include_str!(...)), ...]` com os arquivos `*.ext`
/// da pasta.
fn juntar(pasta: &str, ext: &str, saida: &str) {
    println!("cargo:rerun-if-changed={pasta}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(pasta);
    let mut arquivos: Vec<_> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect())
        .unwrap_or_default();
    arquivos.sort();
    let mut out = String::from("&[\n");
    for p in arquivos {
        println!("cargo:rerun-if-changed={}", p.display());
        let nome = p.file_name().unwrap().to_string_lossy().to_string();
        out.push_str(&format!("    ({nome:?}, include_str!({:?})),\n", p.display().to_string()));
    }
    out.push(']');
    let alvo = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join(saida);
    std::fs::write(alvo, out).unwrap();
}
