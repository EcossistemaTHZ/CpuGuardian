#[derive(Debug, Default, PartialEq)]
pub enum Decision { #[default] Hold, Throttle, Restore }

#[derive(Default)]
pub struct Governor { high_count: u32, low_count: u32, throttled: bool }

impl Governor {
    pub fn update(&mut self, cpu: f32, high: f32, low: f32, high_samples: u32, low_samples: u32) -> Decision {
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
            self.low_count = if cpu <= low { self.low_count + 1 } else { 0 };
            if self.low_count >= low_samples {
                self.throttled = false;
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
    fn hysteresis_prevents_flapping() {
        let mut g = Governor::default();
        assert_eq!(g.update(90.0, 80.0, 60.0, 2, 2), Decision::Hold);
        assert_eq!(g.update(90.0, 80.0, 60.0, 2, 2), Decision::Throttle);
        assert_eq!(g.update(50.0, 80.0, 60.0, 2, 2), Decision::Hold);
        assert_eq!(g.update(70.0, 80.0, 60.0, 2, 2), Decision::Hold);
        assert_eq!(g.update(50.0, 80.0, 60.0, 2, 2), Decision::Hold);
        assert_eq!(g.update(50.0, 80.0, 60.0, 2, 2), Decision::Restore);
    }
}

