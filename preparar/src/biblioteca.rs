//! A biblioteca do Preparar: as pastas do computador e do pendrive, as músicas de cada pasta e o
//! que se faz com elas (copiar para o pendrive já analisando, mover, criar pasta,
//! renomear, apagar). Sem tela: dá para testar.

use dj_os_analise::prepare::is_audio;
use std::path::{Path, PathBuf};

/// De onde é a pasta.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lado {
    Pc,
    Pendrive,
}

/// Uma música da lista.
#[derive(Clone, Debug)]
pub struct Musica {
    pub caminho: PathBuf,
    pub titulo: String,
    pub artista: String,
    pub bpm: Option<f64>,
    pub tom: Option<u8>,
    /// Já tem a análise guardada (no pendrive, a dele; no PC, a do PC).
    pub analisada: bool,
}

fn nome(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

/// Pasta que não aparece: oculta, do sistema ou a da análise do DJ OS.
fn escondida(p: &Path) -> bool {
    let n = nome(p);
    n.starts_with('.') || n.starts_with('$') || n == "System Volume Information" || n == "DJOS" || n == "PIONEER"
}

/// Subpastas, em ordem alfabética.
pub fn subpastas(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !escondida(p)).collect()).unwrap_or_default();
    v.sort_by_key(|p| nome(p).to_lowercase());
    v
}

/// Músicas da pasta (sem as das subpastas), com o que já se sabe delas.
pub fn musicas(p: &Path, raiz_pendrive: Option<&Path>) -> Vec<Musica> {
    let mut v: Vec<Musica> = std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| is_audio(p)).map(|f| musica(&f, raiz_pendrive)).collect())
        .unwrap_or_default();
    v.sort_by_key(|m| (m.artista.to_lowercase(), m.titulo.to_lowercase()));
    v
}

pub fn musica(f: &Path, raiz_pendrive: Option<&Path>) -> Musica {
    let (titulo, artista) = dj_os_analise::track::names_from_path(f);
    let peek = dj_os_analise::store::peek(f);
    let analisada = match raiz_pendrive {
        Some(r) => dj_os_analise::store::has_portable(r, f),
        None => peek.as_ref().is_some_and(|p| p.completa()),
    };
    Musica {
        caminho: f.to_path_buf(),
        titulo,
        artista,
        bpm: peek.as_ref().map(|p| p.bpm).filter(|b| *b > 0.0),
        tom: peek.and_then(|p| dj_os_analise::key::from_raw(p.key)),
        analisada,
    }
}

/// Uma linha da lista de um lado: voltar, uma subpasta ou uma música.
#[derive(Clone, Debug)]
pub enum Item {
    Voltar,
    Pasta(PathBuf),
    Musica(Musica),
}

impl Item {
    pub fn caminho(&self) -> Option<&Path> {
        match self {
            Item::Voltar => None,
            Item::Pasta(p) => Some(p),
            Item::Musica(m) => Some(&m.caminho),
        }
    }
}

/// O que aparece numa pasta: "← Voltar" (fora do começo do disco), as subpastas e as músicas.
pub fn conteudo(pasta: &Path, raiz: &Path, raiz_pendrive: Option<&Path>) -> Vec<Item> {
    let voltar = (pasta != raiz).then_some(Item::Voltar);
    voltar.into_iter().chain(subpastas(pasta).into_iter().map(Item::Pasta)).chain(musicas(pasta, raiz_pendrive).into_iter().map(Item::Musica)).collect()
}

/// Lugares do computador para escolher: pasta pessoal, músicas e os discos (internos e
/// externos). Nome para mostrar e caminho.
pub fn discos() -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(h) = &home {
        if let Some(m) = ["Música", "Music", "Músicas"].iter().map(|n| h.join(n)).find(|p| p.is_dir()) {
            v.push(("Músicas".to_string(), m));
        }
        v.push(("Pasta pessoal".to_string(), h.clone()));
    }
    if cfg!(windows) {
        for c in b'C'..=b'Z' {
            let p = PathBuf::from(format!("{}:\\", c as char));
            if p.is_dir() {
                v.push((format!("Disco {}:", c as char), p));
            }
        }
    } else {
        let user = std::env::var("USER").unwrap_or_default();
        for base in [PathBuf::from("/mnt"), PathBuf::from("/media").join(&user), PathBuf::from("/run/media").join(&user)] {
            for d in subpastas(&base) {
                if std::fs::read_dir(&d).is_ok() {
                    v.push((format!("Disco {}", nome(&d)), d));
                }
            }
        }
        v.push(("Computador inteiro (/)".to_string(), PathBuf::from("/")));
    }
    v
}

/// Tira o pendrive com segurança (desmonta e desliga). Devolve o erro em português.
pub fn ejetar(raiz: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let letra = raiz.display().to_string().trim_end_matches('\\').to_string();
        let cmd = format!("(New-Object -comObject Shell.Application).Namespace(17).ParseName('{letra}').InvokeVerb('Eject')");
        let ok = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &cmd]).status().is_ok_and(|s| s.success());
        return if ok { Ok(()) } else { Err("o Windows não deixou ejetar (feche o que estiver usando o pendrive)".into()) };
    }
    #[cfg(not(windows))]
    {
        let saida = std::process::Command::new("findmnt").args(["-n", "-o", "SOURCE", "--target"]).arg(raiz).output().map_err(|e| e.to_string())?;
        let dev = String::from_utf8_lossy(&saida.stdout).trim().to_string();
        if dev.is_empty() {
            return Err("não achei o pendrive".into());
        }
        let des = std::process::Command::new("udisksctl").args(["unmount", "-b", &dev]).output().map_err(|e| e.to_string())?;
        if !des.status.success() {
            return Err("ainda tem algo usando o pendrive (feche e tente de novo)".into());
        }
        // Desligar é bom (a luz apaga), mas se não der o pendrive já pode sair.
        let _ = std::process::Command::new("udisksctl").args(["power-off", "-b", &dev]).output();
        Ok(())
    }
}

/// Nome de arquivo ou pasta que dá para usar no pendrive (FAT/exFAT recusam alguns caracteres).
pub fn nome_valido(n: &str) -> Option<String> {
    let n: String = n.trim().chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let n = n.trim_end_matches(['.', ' ']).to_string();
    (!n.is_empty() && n != "." && n != "..").then_some(n)
}

/// Caminho livre: se já existe, acrescenta " (2)", " (3)"...
pub fn livre(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| p.with_file_name(format!("{stem} ({n}){ext}"))).find(|c| !c.exists()).expect("nome livre")
}

/// Cria uma pasta nova dentro de `pai`; devolve o caminho.
pub fn criar_pasta(pai: &Path, nome: &str) -> std::io::Result<PathBuf> {
    let n = nome_valido(nome).ok_or_else(|| std::io::Error::other("nome vazio"))?;
    let p = livre(&pai.join(n));
    std::fs::create_dir(&p)?;
    Ok(p)
}

/// Renomeia uma pasta ou música (no mesmo lugar); devolve o caminho novo.
pub fn renomear(p: &Path, novo: &str) -> std::io::Result<PathBuf> {
    let mut n = nome_valido(novo).ok_or_else(|| std::io::Error::other("nome vazio"))?;
    // Música: mantém a extensão se a pessoa não escreveu.
    if p.is_file() {
        if let Some(ext) = p.extension().map(|e| e.to_string_lossy().to_string()) {
            if !n.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase())) {
                n = format!("{n}.{ext}");
            }
        }
    }
    let alvo = p.with_file_name(n);
    if alvo == p {
        return Ok(alvo);
    }
    let alvo = livre(&alvo);
    std::fs::rename(p, &alvo)?;
    Ok(alvo)
}

/// Uma cópia a fazer: de onde, para onde (o arquivo final).
#[derive(Clone, Debug, PartialEq)]
pub struct Copia {
    pub de: PathBuf,
    pub para: PathBuf,
}

/// O que copiar para levar `itens` (músicas ou pastas inteiras) para dentro de `destino`: as
/// músicas e as subpastas com músicas, mantendo a estrutura. Música que já existe lá com o mesmo
/// tamanho é pulada (copiar de novo não duplica).
pub fn planejar(itens: &[PathBuf], destino: &Path) -> Vec<Copia> {
    let mut out = Vec::new();
    for item in itens {
        if item.is_dir() {
            // Não copia uma pasta para dentro dela mesma.
            if destino.starts_with(item) {
                continue;
            }
            let base = destino.join(nome(item));
            let mut pilha = vec![item.clone()];
            while let Some(d) = pilha.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() && !escondida(&p) {
                        pilha.push(p);
                    } else if is_audio(&p) {
                        let rel = p.strip_prefix(item).unwrap_or(&p);
                        out.push(Copia { de: p.clone(), para: base.join(rel) });
                    }
                }
            }
        } else if is_audio(item) {
            out.push(Copia { de: item.clone(), para: destino.join(nome(item)) });
        }
    }
    out.retain(|c| !(c.para.exists() && tamanho(&c.para) == tamanho(&c.de)));
    out.sort_by(|a, b| a.para.cmp(&b.para));
    out
}

fn tamanho(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

/// Copia um arquivo com segurança: grava como ".parcial", confere o tamanho e só então dá o nome
/// final (um pendrive tirado no meio não deixa música pela metade com o nome certo).
pub fn copiar(c: &Copia) -> std::io::Result<PathBuf> {
    if let Some(pai) = c.para.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let final_ = if c.para.exists() { livre(&c.para) } else { c.para.clone() };
    let parcial = final_.with_file_name(format!("{}.parcial", nome(&final_)));
    let n = std::fs::copy(&c.de, &parcial)?;
    let f = std::fs::File::open(&parcial)?;
    f.sync_all()?;
    if Some(n) != tamanho(&c.de) || Some(n) != tamanho(&parcial) {
        let _ = std::fs::remove_file(&parcial);
        return Err(std::io::Error::other("cópia incompleta"));
    }
    std::fs::rename(&parcial, &final_)?;
    Ok(final_)
}

/// Move músicas ou pastas para dentro de `destino` (no mesmo pendrive: só muda de lugar).
pub fn mover(itens: &[PathBuf], destino: &Path) -> std::io::Result<usize> {
    let mut n = 0;
    for item in itens {
        if destino.starts_with(item) || item.parent() == Some(destino) {
            continue;
        }
        let alvo = livre(&destino.join(nome(item)));
        std::fs::rename(item, &alvo)?;
        n += 1;
    }
    Ok(n)
}

/// Apaga músicas ou pastas (do pendrive). As pastas vão com tudo o que tem dentro.
pub fn apagar(itens: &[PathBuf]) -> std::io::Result<()> {
    for item in itens {
        if item.is_dir() {
            std::fs::remove_dir_all(item)?;
        } else {
            std::fs::remove_file(item)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(nome: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("preparar-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn arquivo(p: &Path, conteudo: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, conteudo).unwrap();
    }

    #[test]
    fn conteudo_com_voltar_pastas_e_musicas() {
        let pc = pasta("conteudo");
        for d in ["Funk/Antigas", "House", ".oculta", "DJOS/analise"] {
            std::fs::create_dir_all(pc.join(d)).unwrap();
        }
        arquivo(&pc.join("Funk/MC A - Um.mp3"), b"um");
        arquivo(&pc.join("Funk/capa.jpg"), b"x");
        let raiz: Vec<_> = conteudo(&pc, &pc, None).iter().map(|i| format!("{i:?}").split('(').next().unwrap().to_string()).collect();
        assert_eq!(raiz, ["Pasta", "Pasta"], "no começo do disco não tem voltar; ocultas e a do DJ OS ficam de fora");
        let funk = conteudo(&pc.join("Funk"), &pc, None);
        assert!(matches!(funk[0], Item::Voltar));
        assert!(matches!(&funk[1], Item::Pasta(p) if p.ends_with("Antigas")));
        assert!(matches!(&funk[2], Item::Musica(m) if m.titulo == "Um" && m.artista == "MC A"));
        assert_eq!(funk.len(), 3, "a capa não entra");
        let _ = std::fs::remove_dir_all(pc);
    }

    #[test]
    fn copia_mantem_as_pastas_e_nao_duplica() {
        let pc = pasta("copia-pc");
        let pd = pasta("copia-pd");
        arquivo(&pc.join("Set/MC A - Um.mp3"), b"um");
        arquivo(&pc.join("Set/Lentas/MC B - Dois.wav"), b"dois");
        arquivo(&pc.join("Set/capa.jpg"), b"x");
        arquivo(&pc.join("Solta - Tres.flac"), b"tres");
        let plano = planejar(&[pc.join("Set"), pc.join("Solta - Tres.flac")], &pd);
        let destinos: Vec<_> = plano.iter().map(|c| c.para.strip_prefix(&pd).unwrap().to_path_buf()).collect();
        assert_eq!(destinos, [PathBuf::from("Set/Lentas/MC B - Dois.wav"), PathBuf::from("Set/MC A - Um.mp3"), PathBuf::from("Solta - Tres.flac")], "sem a capa");
        for c in &plano {
            copiar(c).unwrap();
        }
        assert_eq!(std::fs::read(pd.join("Set/Lentas/MC B - Dois.wav")).unwrap(), b"dois");
        assert!(!pd.join("Set/MC A - Um.mp3.parcial").exists());
        assert!(planejar(&[pc.join("Set")], &pd).is_empty(), "copiar de novo não duplica");
        assert!(planejar(&[pd.join("Set")], &pd.join("Set/Lentas")).is_empty(), "pasta para dentro dela mesma, não");
        let _ = std::fs::remove_dir_all(pc);
        let _ = std::fs::remove_dir_all(pd);
    }

    #[test]
    fn criar_renomear_mover_e_apagar() {
        let pd = pasta("ops");
        let nova = criar_pasta(&pd, "Baile: Sábado?").unwrap();
        assert_eq!(nome(&nova), "Baile_ Sábado_", "caracteres que o pendrive não aceita viram _");
        assert_eq!(nome(&criar_pasta(&pd, "Baile: Sábado?").unwrap()), "Baile_ Sábado_ (2)", "não sobrescreve");
        assert!(criar_pasta(&pd, "   ").is_err());
        arquivo(&pd.join("MC A - Um.mp3"), b"um");
        let r = renomear(&pd.join("MC A - Um.mp3"), "MC A - Um (remix)").unwrap();
        assert_eq!(nome(&r), "MC A - Um (remix).mp3", "mantém a extensão");
        assert_eq!(mover(&[r.clone()], &nova).unwrap(), 1);
        assert!(nova.join("MC A - Um (remix).mp3").exists());
        assert_eq!(mover(&[nova.clone()], &nova.join("x")).unwrap(), 0, "pasta para dentro dela mesma, não");
        apagar(&[nova.clone()]).unwrap();
        assert!(!nova.exists());
        let _ = std::fs::remove_dir_all(pd);
    }
}
