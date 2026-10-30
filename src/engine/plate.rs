//! Reverb de placa (Dattorro, "Effect Design Part 1", JAES 1997), na versão do CAPS "PlateX2" de
//! Tim Goetze (GPL-3.0-or-later), a mesma usada pelo reverb do Mixxx. Constantes iguais às do CAPS;
//! o código foi reescrito em Rust.
//!
//! Entrada mono, saída estéreo só com o som do reverb (quem chama soma com o som seco).

/// Linha de atraso com tamanho em potência de 2. `get` devolve a amostra de `n` passos atrás.
struct Delay {
    data: Vec<f32>,
    mask: usize,
    read: usize,
    write: usize,
}

impl Delay {
    fn new(n: usize) -> Self {
        let size = (n + 1).next_power_of_two();
        Self { data: vec![0.0; size], mask: size - 1, read: 0, write: n }
    }

    #[inline]
    fn get(&mut self) -> f32 {
        let x = self.data[self.read];
        self.read = (self.read + 1) & self.mask;
        x
    }

    #[inline]
    fn put(&mut self, x: f32) {
        self.data[self.write] = x;
        self.write = (self.write + 1) & self.mask;
    }

    #[inline]
    fn putget(&mut self, x: f32) -> f32 {
        self.put(x);
        self.get()
    }

    /// Amostra de `i` passos atrás (1 = a última gravada).
    #[inline]
    fn tap(&self, i: usize) -> f32 {
        self.data[self.write.wrapping_sub(i) & self.mask]
    }

    /// Leitura fracionária (interpolação linear).
    #[inline]
    fn get_linear(&self, d: f32) -> f32 {
        let n = d as usize;
        let f = d - n as f32;
        (1.0 - f) * self.tap(n) + f * self.tap(n + 1)
    }

    fn clear(&mut self) {
        self.data.iter_mut().for_each(|s| *s = 0.0);
    }
}

/// Passa-tudo (difusor).
struct Lattice(Delay);

impl Lattice {
    #[inline]
    fn process(&mut self, x: f32, d: f32) -> f32 {
        let y = self.0.get();
        let x = x - d * y;
        self.0.put(x);
        d * x + y
    }
}

/// Passa-tudo com o atraso variando devagar (tira o som metálico da cauda).
struct ModLattice {
    delay: Delay,
    n0: f32,
    width: f32,
    phase: f32,
    step: f32,
}

impl ModLattice {
    #[inline]
    fn process(&mut self, x: f32, d: f32) -> f32 {
        self.phase += self.step;
        if self.phase > std::f32::consts::TAU {
            self.phase -= std::f32::consts::TAU;
        }
        let y = self.delay.get_linear(self.n0 + self.width * self.phase.sin());
        let x = x + d * y;
        self.delay.put(x);
        y - d * x
    }
}

/// Filtro passa-baixas de 1 polo: y = a·x + (1 − a)·y.
#[derive(Default)]
struct Lp1 {
    a: f32,
    y: f32,
}

impl Lp1 {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        self.y = self.a * x + (1.0 - self.a) * self.y;
        self.y
    }
}

const LENGTHS: [f32; 12] = [
    0.004771345, 0.003595309, 0.012734787, 0.009307483, // difusores da entrada
    0.022579886, 0.030509727, // passa-tudos modulados (esquerda, direita)
    0.149625349, 0.060481839, 0.124995800, // esquerda: atraso, passa-tudo, atraso
    0.141695508, 0.089244313, 0.106280031, // direita: atraso, passa-tudo, atraso
];
const TAPS: [f32; 12] = [
    0.008937872, 0.099929438, 0.064278754, 0.067067639, 0.066866033, 0.006283391, 0.011861161, 0.121870905, 0.041262054,
    0.089815530, 0.070931756, 0.011256342,
];
const IN_DIFF1: f32 = 0.742;
const IN_DIFF2: f32 = 0.712;
const DE_DIFF1: f32 = 0.723;
const DE_DIFF2: f32 = 0.729;

pub struct Plate {
    bandwidth: Lp1,
    input: [Lattice; 4],
    mlattice: [ModLattice; 2],
    lattice: [Lattice; 2],
    delay: [Delay; 4],
    damping: [Lp1; 2],
    taps: [usize; 12],
    /// Desvio mínimo alternado contra números "denormais" (deixam o processador lento no silêncio).
    tiny: f32,
}

impl Plate {
    pub fn new(fs: f32) -> Self {
        let n = |i: usize| (LENGTHS[i] * fs) as usize;
        let width = (0.000403221 * fs) as usize;
        let lfo = |i: usize, phase: f32| ModLattice {
            delay: Delay::new(n(i) + width + 2),
            n0: n(i) as f32,
            width: width as f32,
            phase,
            step: std::f32::consts::TAU * 1.2 / fs,
        };
        let mut p = Self {
            bandwidth: Lp1::default(),
            input: [0, 1, 2, 3].map(|i| Lattice(Delay::new(n(i)))),
            mlattice: [lfo(4, 0.0), lfo(5, std::f32::consts::FRAC_PI_2)],
            lattice: [Lattice(Delay::new(n(7))), Lattice(Delay::new(n(10)))],
            delay: [Delay::new(n(6)), Delay::new(n(8)), Delay::new(n(9)), Delay::new(n(11))],
            damping: [Lp1::default(), Lp1::default()],
            taps: TAPS.map(|t| (t * fs) as usize),
            tiny: 1e-20,
        };
        p.set_bandwidth(0.9);
        p.set_damping(0.2);
        p
    }

    /// Brilho da entrada (0 = abafado, 1 = aberto).
    pub fn set_bandwidth(&mut self, bw: f32) {
        self.bandwidth.a = (-std::f32::consts::PI * (1.0 - (0.005 + 0.994 * bw))).exp();
    }

    /// Quanto a cauda perde de agudo (0 = brilhante, 1 = abafada).
    pub fn set_damping(&mut self, dm: f32) {
        let a = (-std::f32::consts::PI * (0.0005 + 0.9995 * dm)).exp();
        self.damping.iter_mut().for_each(|d| d.a = a);
    }

    pub fn clear(&mut self) {
        self.bandwidth.y = 0.0;
        for l in self.input.iter_mut().chain(self.lattice.iter_mut()) {
            l.0.clear();
        }
        for m in self.mlattice.iter_mut() {
            m.delay.clear();
        }
        for d in self.delay.iter_mut() {
            d.clear();
        }
        self.damping.iter_mut().for_each(|d| d.y = 0.0);
    }

    /// Uma amostra: `x` mono, `decay` de 0 a ~0,89 (tamanho da cauda). Devolve [esquerda, direita].
    #[inline]
    pub fn process(&mut self, x: f32, decay: f32) -> [f32; 2] {
        self.tiny = -self.tiny;
        let mut x = self.bandwidth.process(x + self.tiny);
        x = self.input[0].process(x, IN_DIFF1);
        x = self.input[1].process(x, IN_DIFF1);
        x = self.input[2].process(x, IN_DIFF2);
        x = self.input[3].process(x, IN_DIFF2);

        let mut xl = x + decay * self.delay[3].get();
        let mut xr = x + decay * self.delay[1].get();

        xl = self.mlattice[0].process(xl, DE_DIFF1);
        xl = self.delay[0].putget(xl);
        xl = self.damping[0].process(xl) * decay;
        xl = self.lattice[0].process(xl, DE_DIFF2);
        self.delay[1].put(xl);

        xr = self.mlattice[1].process(xr, DE_DIFF1);
        xr = self.delay[2].putget(xr);
        xr = self.damping[1].process(xr) * decay;
        xr = self.lattice[1].process(xr, DE_DIFF2);
        self.delay[3].put(xr);

        let t = &self.taps;
        let (d, l) = (&self.delay, &self.lattice);
        let left = 0.6 * (d[2].tap(t[0]) + d[2].tap(t[1]) - l[1].0.tap(t[2]) + d[3].tap(t[3]) - d[0].tap(t[4]) + l[0].0.tap(t[5]));
        let right = 0.6 * (d[0].tap(t[6]) + d[0].tap(t[7]) - l[0].0.tap(t[8]) + d[1].tap(t[9]) - d[2].tap(t[10]) + l[1].0.tap(t[11]));
        [left, right]
    }
}
