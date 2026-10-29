//! Keylock: muda o andamento sem mudar o tom (WSOLA).
//!
//! A música é tocada em pedaços curtos ("grãos" de ~43 ms) na velocidade normal, com janela e
//! sobreposição de 50%. O andamento vem de onde cada grão começa: a posição do deck, que anda
//! na velocidade do pitch. Para não dar falha, cada grão novo é ajustado (até ±11 ms) para o ponto
//! em que mais se parece com a continuação do grão anterior. A busca usa o sinal reduzido
//! (1 a cada 8 amostras) para ficar leve numa máquina antiga.

use crate::track::Track;

/// Tamanho do grão e do passo (em amostras de saída).
const N: usize = 2048;
const H: usize = N / 2;
/// Busca do encaixe: ±512 amostras da música, de 4 em 4.
const SEARCH: i64 = 512;
const SEARCH_STEP: i64 = 4;
const DECIM: usize = 8;

pub struct Keylock {
    window: Vec<f32>,
    acc: Vec<[f32; 2]>,
    idx: usize,
    prev_start: Option<f64>,
    target: Vec<f32>,
}

impl Keylock {
    pub fn new() -> Self {
        // Hann periódica: com 50% de sobreposição as janelas somam exatamente 1.
        let window = (0..N).map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / N as f32).cos()).collect();
        Self { window, acc: vec![[0.0; 2]; N], idx: 0, prev_start: None, target: vec![0.0; H / DECIM] }
    }

    /// Esquece o grão anterior (depois de pular, carregar música ou sair do scratch).
    pub fn reset(&mut self) {
        self.acc.iter_mut().for_each(|s| *s = [0.0; 2]);
        self.idx = 0;
        self.prev_start = None;
    }

    #[inline]
    fn mono(t: &Track, pos: f64) -> f32 {
        let (l, r) = t.sample(pos);
        l + r
    }

    /// Melhor início para o grão novo perto de `nominal`, comparando com a continuação do anterior.
    fn search(&mut self, t: &Track, cont: f64, nominal: f64, ratio: f64) -> f64 {
        let step = ratio * DECIM as f64;
        for (k, v) in self.target.iter_mut().enumerate() {
            *v = Self::mono(t, cont + k as f64 * step);
        }
        let mut best = (f32::MIN, nominal);
        let mut d = -SEARCH;
        while d <= SEARCH {
            let s = nominal + d as f64;
            let (mut xy, mut yy) = (0.0f32, 1e-9f32);
            for (k, x) in self.target.iter().enumerate() {
                let y = Self::mono(t, s + k as f64 * step);
                xy += x * y;
                yy += y * y;
            }
            let score = xy / yy.sqrt();
            if score > best.0 {
                best = (score, s);
            }
            d += SEARCH_STEP;
        }
        best.1
    }

    /// Próxima amostra de saída. `pos` é a posição atual do deck (em quadros da música) e
    /// `ratio` = taxa da música / taxa de saída.
    #[inline]
    pub fn next(&mut self, t: &Track, pos: f64, ratio: f64) -> (f32, f32) {
        if self.idx == 0 {
            let start = match self.prev_start {
                Some(prev) => self.search(t, prev + H as f64 * ratio, pos, ratio),
                None => pos,
            };
            for n in 0..N {
                let (l, r) = t.sample(start + n as f64 * ratio);
                let w = self.window[n];
                self.acc[n][0] += l * w;
                self.acc[n][1] += r * w;
            }
            self.prev_start = Some(start);
        }
        let o = self.acc[self.idx];
        self.idx += 1;
        if self.idx == H {
            self.acc.copy_within(H.., 0);
            self.acc[H..].iter_mut().for_each(|s| *s = [0.0; 2]);
            self.idx = 0;
        }
        (o[0], o[1])
    }
}
