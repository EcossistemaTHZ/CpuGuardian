#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Hold,
    Throttle,
    PanicFreeze,
    Restore,
}

#[derive(Default)]
pub struct Governor {
    high_count: u32,
    panic_count: u32,
    low_count: u32,
    throttled: bool,
    frozen: bool,
}

impl Governor {
    pub fn update(
        &mut self,
        cpu: f32,
        mem_crit: bool,
        high: f32,
        panic: f32,
        low: f32,
        high_samples: u32,
        panic_samples: u32,
        low_samples: u32,
    ) -> Decision {
        // Se a memória estiver crítica ou CPU ultrapassar o limiar de pânico
        let is_panic_condition = cpu >= panic || mem_crit;

        if is_panic_condition {
            self.panic_count += 1;
            if self.panic_count >= panic_samples && !self.frozen {
                self.frozen = true;
                self.throttled = true;
                return Decision::PanicFreeze;
            }
        } else {
            self.panic_count = 0;
        }

        if !self.throttled {
            self.low_count = 0;
            self.high_count = if cpu >= high { self.high_count + 1 } else { 0 };
            if self.high_count >= high_samples {
                self.throttled = true;
                self.high_count = 0;
                return Decision::Throttle;
            }
        } else {
            self.high_count = 0;
            self.low_count = if cpu <= low && !mem_crit { self.low_count + 1 } else { 0 };
            if self.low_count >= low_samples {
                self.throttled = false;
                self.frozen = false;
                self.low_count = 0;
                return Decision::Restore;
            }
        }

        Decision::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_escalation() {
        let mut g = Governor::default();
        // Carga alta comum: aciona Throttle
        assert_eq!(g.update(85.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::Hold);
        assert_eq!(g.update(85.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::Throttle);

        // Carga sobe para pânico (>95%): aciona PanicFreeze
        assert_eq!(g.update(98.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::Hold);
        assert_eq!(g.update(98.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::PanicFreeze);

        // Baixa carga: restaura
        assert_eq!(g.update(50.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::Hold);
        assert_eq!(g.update(50.0, false, 80.0, 95.0, 60.0, 2, 2, 2), Decision::Restore);
    }
}
