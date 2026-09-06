#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Hold,
    Throttle,
    ThermalLimit,
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
    thermal_count: u32,
}

impl Governor {
    pub fn update(
        &mut self,
        cpu: f32,
        mem_crit: bool,
        temperature: Option<f32>,
        high: f32,
        panic: f32,
        low: f32,
        high_samples: u32,
        panic_samples: u32,
        low_samples: u32,
        thermal_high: f32,
        thermal_panic: f32,
        thermal_emergency: f32,
    ) -> Decision {
        let hot = temperature.is_some_and(|t| t >= thermal_high);
        let thermal_panic_condition = temperature.is_some_and(|t| t >= thermal_panic);
        let emergency = mem_crit || temperature.is_some_and(|t| t >= thermal_emergency);
        // Sem sensor, CPU extrema ainda aciona limite progressivo, nunca SIGSTOP.
        let is_panic_condition = thermal_panic_condition || (temperature.is_none() && cpu >= panic);

        if emergency && !self.frozen {
            self.frozen = true;
            self.throttled = true;
            return Decision::PanicFreeze;
        }

        if is_panic_condition {
            self.panic_count += 1;
            if self.panic_count >= panic_samples && !self.frozen {
                self.throttled = true;
                return Decision::ThermalLimit;
            }
        } else {
            self.panic_count = 0;
        }

        self.thermal_count = if hot { self.thermal_count + 1 } else { 0 };
        if self.thermal_count >= high_samples && !self.frozen {
            self.throttled = true;
            self.thermal_count = 0;
            return Decision::ThermalLimit;
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
            let cool = temperature.is_none_or(|t| t < thermal_high - 5.0);
            self.low_count = if cpu <= low && !mem_crit && cool {
                self.low_count + 1
            } else {
                0
            };
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
        let args = (80.0, 95.0, 60.0, 2, 2, 2, 82.0, 90.0, 97.0);
        assert_eq!(
            g.update(
                85.0,
                false,
                Some(70.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::Hold
        );
        assert_eq!(
            g.update(
                85.0,
                false,
                Some(70.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::Throttle
        );

        // Carga sobe para pânico (>95%): aciona PanicFreeze
        assert_eq!(
            g.update(
                98.0,
                false,
                Some(91.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::Hold
        );
        assert_eq!(
            g.update(
                98.0,
                false,
                Some(91.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::ThermalLimit
        );

        // Baixa carga: restaura
        assert_eq!(
            g.update(
                50.0,
                false,
                Some(70.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::Hold
        );
        assert_eq!(
            g.update(
                50.0,
                false,
                Some(70.0),
                args.0,
                args.1,
                args.2,
                args.3,
                args.4,
                args.5,
                args.6,
                args.7,
                args.8
            ),
            Decision::Restore
        );
    }
}
