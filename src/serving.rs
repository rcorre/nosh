use std::str::FromStr;

use anyhow::Context as _;

// Serving is a portion of food, optionally paired with a unit.
// Without a unit, it represents a portion of a serving, e.g. 1.5 servings.
// Otherwise, it represents a quantity such as "150 grams".
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Serving {
    pub size: f32,
    pub unit: Option<String>,
}

impl Default for Serving {
    fn default() -> Self {
        Self {
            size: 1.0,
            unit: None,
        }
    }
}

impl std::ops::Mul<f32> for Serving {
    type Output = Serving;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            size: self.size * rhs,
            unit: self.unit,
        }
    }
}

impl std::fmt::Display for Serving {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.unit {
            Some(unit) => write!(f, "{} {}", self.size, unit),
            None => write!(f, "{}", self.size),
        }
    }
}

// A serving can be parsed from a strings of form:
// "1.5", "1.5cups", "1.5 cups"
impl FromStr for Serving {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        let s = s.trim();
        let (size, unit) = match s.find(|c: char| c != '.' && !c.is_digit(10)) {
            Some(idx) => {
                let (size, unit) = s.split_at(idx);
                (size.trim(), Some(unit.trim()))
            }
            None => (s, None),
        };
        let size = size.parse().with_context(|| format!("Parsing '{size}'"))?;
        Ok(Self {
            size,
            unit: unit.map(str::to_string),
        })
    }
}

#[test]
fn test_parse_serving() {
    let parse = |s: &str| s.parse::<Serving>();
    let serv = |size, unit: Option<&str>| Serving {
        size,
        unit: unit.map(str::to_string),
    };
    assert_eq!(parse("1.5").unwrap(), serv(1.5, None));
    assert_eq!(parse("1.5c").unwrap(), serv(1.5, Some("c")));
    assert_eq!(parse("1.5 cup").unwrap(), serv(1.5, Some("cup")));
    assert_eq!(parse(" 1.5  cup ").unwrap(), serv(1.5, Some("cup")));
    assert_eq!(parse("25 g dry").unwrap(), serv(25.0, Some("g dry")));
    assert_eq!(parse("25g dry").unwrap(), serv(25.0, Some("g dry")));
    assert!(parse("cup 1.5").is_err());
}
