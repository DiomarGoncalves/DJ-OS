//! Teclado virtual da pesquisa (estilo XDJ).
//!
//! Segurar o BROWSE abre o browser em tela cheia com o teclado. Girar o BROWSE anda pelas teclas
//! e apertar digita; os pads também navegam (1 ↑, 2 ↓, 3 ←, 4 →, 5 confirmar, 6 apagar,
//! 7 espaço, 8 fechar). ENTER pesquisa em todas as unidades e mostra os resultados na lista.

use crate::app::AppState;

pub const ROWS: [&[&str]; 5] = [
    &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
    &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"],
    &["A", "S", "D", "F", "G", "H", "J", "K", "L"],
    &["Z", "X", "C", "V", "B", "N", "M"],
    &["ESPAÇO", "APAGAR", "LIMPAR", "ENTER", "FECHAR"],
];

/// Teclado da senha do Wi-Fi: minúsculas (tecla "Aa") e símbolos.
pub const ROWS_PASS: [&[&str]; 6] = [
    &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
    &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"],
    &["A", "S", "D", "F", "G", "H", "J", "K", "L"],
    &["Z", "X", "C", "V", "B", "N", "M", ".", "-", "_"],
    &["@", "!", "#", "$", "%", "&", "*", "+", "/", "?"],
    &["Aa", "ESPAÇO", "APAGAR", "LIMPAR", "ENTER", "FECHAR"],
];

/// Para que o teclado está aberto.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Purpose {
    #[default]
    Search,
    WifiPassword(String),
}

#[derive(Clone, Debug, Default)]
pub struct SearchUi {
    pub query: String,
    pub row: usize,
    pub col: usize,
    pub purpose: Purpose,
    /// Letras minúsculas (só no teclado da senha).
    pub lower: bool,
}

impl SearchUi {
    pub fn rows(&self) -> &'static [&'static [&'static str]] {
        match self.purpose {
            Purpose::Search => &ROWS,
            Purpose::WifiPassword(_) => &ROWS_PASS,
        }
    }

    pub fn key(&self) -> &'static str {
        self.rows()[self.row][self.col]
    }

    /// Texto que aparece na tecla (letras em minúscula quando "Aa" está ligado).
    pub fn label(&self, key: &str) -> String {
        if self.lower && key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) {
            key.to_lowercase()
        } else {
            key.to_string()
        }
    }

    /// Setas (pads): move na grade, sem sair dela.
    pub fn move_by(&mut self, drow: i32, dcol: i32) {
        let rows = self.rows();
        self.row = (self.row as i32 + drow).clamp(0, rows.len() as i32 - 1) as usize;
        let len = rows[self.row].len() as i32;
        self.col = (self.col as i32 + dcol).clamp(0, len - 1) as usize;
    }

    /// BROWSE girado: anda tecla por tecla em ordem de leitura, dando a volta no fim.
    pub fn step(&mut self, delta: i32) {
        let rows = self.rows();
        let total: usize = rows.iter().map(|r| r.len()).sum();
        let mut idx = rows[..self.row].iter().map(|r| r.len()).sum::<usize>() + self.col;
        idx = (idx as i64 + delta as i64).rem_euclid(total as i64) as usize;
        for (r, row) in rows.iter().enumerate() {
            if idx < row.len() {
                self.row = r;
                self.col = idx;
                return;
            }
            idx -= row.len();
        }
    }

    pub fn go_to(&mut self, key: &str) {
        for (r, row) in self.rows().iter().enumerate() {
            if let Some(c) = row.iter().position(|k| *k == key) {
                self.row = r;
                self.col = c;
            }
        }
    }
}

/// Abre a pesquisa: browser em tela cheia e teclado na tecla ENTER se já houver texto.
pub fn open(app: &mut AppState) {
    if app.search.is_some() {
        return;
    }
    let query = app.lib.results.as_ref().map(|(q, _)| q.clone()).unwrap_or_default();
    let mut ui = SearchUi { query, ..Default::default() };
    if ui.query.is_empty() {
        ui.go_to("A");
    } else {
        ui.go_to("ENTER");
    }
    app.search = Some(ui);
    app.big_browser = true;
}

pub fn close(app: &mut AppState) {
    app.search = None;
}

/// Digita/aciona uma tecla pelo nome.
pub fn press_key(app: &mut AppState, key: &str) {
    let Some(ui) = app.search.as_mut() else { return };
    match key {
        "ESPAÇO" => {
            // Na senha o espaço vale sempre; na pesquisa não repete nem começa com espaço.
            if ui.purpose != Purpose::Search || (!ui.query.ends_with(' ') && !ui.query.is_empty()) {
                ui.query.push(' ');
            }
        }
        "APAGAR" => {
            ui.query.pop();
        }
        "LIMPAR" => ui.query.clear(),
        "Aa" => ui.lower = !ui.lower,
        "FECHAR" => close(app),
        "ENTER" => submit(app),
        k => {
            let max = if ui.purpose == Purpose::Search { 40 } else { 63 };
            if ui.query.chars().count() < max {
                let typed = ui.label(k);
                ui.query.push_str(&typed);
            }
        }
    }
}

/// Aciona a tecla em destaque (BROWSE apertado ou pad 5).
pub fn press(app: &mut AppState) {
    if let Some(key) = app.search.as_ref().map(|u| u.key()) {
        press_key(app, key);
    }
}

/// Abre o teclado para digitar a senha de uma rede Wi-Fi.
pub fn open_password(app: &mut AppState, ssid: &str) {
    let mut ui = SearchUi { purpose: Purpose::WifiPassword(ssid.to_string()), lower: true, ..Default::default() };
    ui.go_to("A");
    app.search = Some(ui);
}

/// ENTER: pesquisa e mostra os resultados na lista (ou conecta no Wi-Fi), fechando o teclado.
pub fn submit(app: &mut AppState) {
    let Some(ui) = app.search.take() else { return };
    if let Purpose::WifiPassword(ssid) = &ui.purpose {
        crate::settings::wifi_connect(app, ssid, &ui.query);
        return;
    }
    let q = ui.query.trim().to_string();
    if q.is_empty() {
        app.search = Some(ui);
        return;
    }
    let n = app.lib.search(&q);
    app.toast(if n == 0 { format!("Nada encontrado para \"{q}\"") } else { format!("{n} música(s) para \"{q}\"") });
}

/// Pads com o teclado aberto: 1 ↑, 2 ↓, 3 ←, 4 →, 5 confirmar, 6 apagar, 7 espaço, 8 fechar.
pub fn pad(app: &mut AppState, idx: usize) {
    let Some(ui) = app.search.as_mut() else { return };
    match idx {
        0 => ui.move_by(-1, 0),
        1 => ui.move_by(1, 0),
        2 => ui.move_by(0, -1),
        3 => ui.move_by(0, 1),
        4 => press(app),
        5 => press_key(app, "APAGAR"),
        6 => press_key(app, "ESPAÇO"),
        7 => close(app),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navegacao_pela_grade_e_pelo_browse() {
        let mut ui = SearchUi::default();
        ui.go_to("Q");
        ui.move_by(1, 0);
        assert_eq!(ui.key(), "A");
        ui.move_by(0, 20);
        assert_eq!(ui.key(), "L", "não passa do fim da linha");
        ui.move_by(1, 0);
        assert_eq!(ui.key(), "M", "linha menor: vai para a última tecla");
        ui.step(1);
        assert_eq!(ui.key(), "ESPAÇO");
        ui.go_to("FECHAR");
        ui.step(1);
        assert_eq!(ui.key(), "1", "dá a volta no fim");
        ui.step(-1);
        assert_eq!(ui.key(), "FECHAR");
    }

    #[test]
    fn digitar_apagar_e_fechar_pelos_pads() {
        let mut app = AppState::new();
        open(&mut app);
        assert!(app.big_browser && app.search.is_some());
        for k in ["M", "C", "ESPAÇO", "ESPAÇO", "K"] {
            press_key(&mut app, k);
        }
        assert_eq!(app.search.as_ref().unwrap().query, "MC K");
        pad(&mut app, 5);
        assert_eq!(app.search.as_ref().unwrap().query, "MC ");
        pad(&mut app, 7);
        assert!(app.search.is_none(), "pad 8 fecha o teclado");
    }

    #[test]
    fn enter_vazio_nao_fecha() {
        let mut app = AppState::new();
        open(&mut app);
        press_key(&mut app, "ENTER");
        assert!(app.search.is_some());
    }
}
