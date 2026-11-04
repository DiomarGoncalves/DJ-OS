//! Browser novo em tela cheia (estilo cabine). A lógica é a mesma do browser de árvore (pastas à
//! esquerda, músicas à direita, BROWSE alterna entre os dois, LOAD abre a pasta ou carrega a
//! música); muda só o desenho, em `ui/principal/navegador.rs`.

use crate::app::AppState;

/// O browser novo está na tela? (CONFIG → TELA, ou DJOS_NAVEGADOR=1 para testar.)
pub fn is_on(a: &AppState) -> bool {
    a.big_browser && (a.settings.browser_novo || std::env::var_os("DJOS_NAVEGADOR").is_some())
}
