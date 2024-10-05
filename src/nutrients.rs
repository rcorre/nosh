fn float0(f: &f32) -> String {
    format!("{:.0}", f)
}

fn float1(f: &f32) -> String {
    format!("{:.1}", f)
}

// The macronutrients of a food.
#[derive(Clone, Copy, Debug, Default, tabled::Tabled)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Nutrients {
    #[tabled(display_with = "float1")]
    pub carb: f32,
    #[tabled(display_with = "float1")]
    pub fat: f32,
    #[tabled(display_with = "float1")]
    pub protein: f32,
    #[tabled(display_with = "float0")]
    pub kcal: f32,
}

impl Nutrients {
    // If kcal is 0, compute it using the Atwater General Calculation:
    // 4*carb + 4*protein + 9*fat.
    // Note that there is a newer system that uses food-specific multipliers:
    // See https://en.wikipedia.org/wiki/Atwater_system#Modified_system.
    pub fn maybe_compute_kcal(self) -> Nutrients {
        Nutrients {
            kcal: if self.kcal > 0.0 {
                self.kcal
            } else {
                self.carb * 4.0 + self.fat * 9.0 + self.protein * 4.0
            },
            ..self
        }
    }
}
impl std::ops::Add<Nutrients> for Nutrients {
    type Output = Nutrients;

    fn add(self, rhs: Nutrients) -> Self::Output {
        Nutrients {
            carb: self.carb + rhs.carb,
            fat: self.fat + rhs.fat,
            protein: self.protein + rhs.protein,
            kcal: self.kcal + rhs.kcal,
        }
    }
}

impl std::ops::Mul<f32> for Nutrients {
    type Output = Nutrients;

    fn mul(self, rhs: f32) -> Self::Output {
        Nutrients {
            carb: self.carb * rhs,
            fat: self.fat * rhs,
            protein: self.protein * rhs,
            kcal: self.kcal * rhs,
        }
    }
}

#[test]
fn test_nutrient_mult() {
    let nut = Nutrients {
        carb: 1.2,
        fat: 2.3,
        protein: 3.1,
        kcal: 124.5,
    } * 2.0;

    assert_eq!(nut.carb, 2.4);
    assert_eq!(nut.fat, 4.6);
    assert_eq!(nut.protein, 6.2);
    assert_eq!(nut.kcal, 249.0);
}

#[test]
fn test_nutrient_kcal_computation() {
    let nut = Nutrients {
        carb: 1.2,
        fat: 2.3,
        protein: 3.1,
        kcal: 0.0,
    }
    .maybe_compute_kcal();

    assert_eq!(nut.carb, 1.2);
    assert_eq!(nut.fat, 2.3);
    assert_eq!(nut.protein, 3.1);
    assert_eq!(nut.kcal, 37.9);
}
