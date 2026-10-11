//! Tng thing placement files, one per map in `Levels/FinalAlbion.wad`.
//! See `docs/formats/tng.md`.

use crate::text::{Statement, statements};
use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tng {
    pub version: u32,
    pub things: Vec<Thing>,
}

/// One `NewThing ... EndThing;` block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Thing {
    /// `Object`, `Marker`, `Thing`, `AICreature`, `TrackNode`, `Building`, `Holy Site`, ...
    pub kind: String,
    /// Value of the enclosing `XXXSectionStart` (usually `NULL`).
    pub section: String,
    /// Statements directly inside the thing, in file order.
    pub fields: Vec<Field>,
    /// `StartCTC...` / `EndCTC...` blocks, in file order.
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Component {
    /// Name without the `Start` prefix, e.g. `CTCPhysicsStandard`.
    pub name: String,
    pub fields: Vec<Field>,
}

/// A statement's key (with any `[arg]`) and its raw value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
}

impl Field {
    fn from(s: &Statement) -> Self {
        let key = match s.arg {
            Some(arg) => format!("{}[{arg}]", s.key),
            None => s.key.to_owned(),
        };
        Self {
            key,
            value: s.value.to_owned(),
        }
    }
}

fn find<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.key == key)
        .map(|f| f.value.as_str())
}

fn unquote(v: &str) -> &str {
    v.strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(v)
}

impl Thing {
    pub fn field(&self, key: &str) -> Option<&str> {
        find(&self.fields, key)
    }

    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.name == name)
    }

    /// `DefinitionType`, e.g. `BUILDING_TOWN_WAREHOUSE_02`.
    pub fn definition(&self) -> Option<&str> {
        self.field("DefinitionType").map(unquote)
    }

    pub fn uid(&self) -> Option<u64> {
        self.field("UID")?.parse().ok()
    }

    /// `ScriptName`, or `None` when it is `NULL`.
    pub fn script_name(&self) -> Option<&str> {
        self.field("ScriptName").filter(|&n| n != "NULL")
    }

    /// Objects and buildings keep their placement in `CTCPhysicsStandard`,
    /// creatures in `CTCPhysicsNavigator` (same fields).
    fn physics_vector(&self, prefix: &str) -> Option<[f32; 3]> {
        let c = self
            .component("CTCPhysicsStandard")
            .or_else(|| self.component("CTCPhysicsNavigator"))?;
        let axis = |a: &str| find(&c.fields, &format!("{prefix}{a}"))?.parse().ok();
        Some([axis("X")?, axis("Y")?, axis("Z")?])
    }

    /// Position relative to the map's origin (its `.wld` `MapX`, `MapY`), z up.
    pub fn position(&self) -> Option<[f32; 3]> {
        self.physics_vector("Position")
    }

    pub fn forward(&self) -> Option<[f32; 3]> {
        self.physics_vector("RHSetForward")
    }

    pub fn up(&self) -> Option<[f32; 3]> {
        self.physics_vector("RHSetUp")
    }
}

impl Tng {
    pub fn parse(text: &str) -> Result<Self> {
        let mut tng = Tng::default();
        let mut section = String::new();
        let mut stmts = statements(text)?.into_iter();
        while let Some(s) = stmts.next() {
            match s.key {
                "Version" => tng.version = s.number()?,
                "XXXSectionStart" => section = s.value.to_owned(),
                "XXXSectionEnd" => section.clear(),
                "NewThing" => {
                    let mut thing = Thing {
                        kind: s.value.to_owned(),
                        section: section.clone(),
                        ..Default::default()
                    };
                    let mut open: Option<Component> = None;
                    loop {
                        let f = stmts.next().ok_or_else(|| s.err("unterminated NewThing"))?;
                        match (f.key, &mut open) {
                            ("EndThing", None) => break,
                            ("EndThing", Some(c)) => {
                                return Err(f.err(format!("EndThing inside Start{}", c.name)));
                            }
                            (k, None) if k.starts_with("StartCTC") => {
                                open = Some(Component {
                                    name: k["Start".len()..].to_owned(),
                                    fields: Vec::new(),
                                });
                            }
                            (k, Some(c)) if k.starts_with("EndCTC") => {
                                if k["End".len()..] != c.name {
                                    return Err(f.err(format!("{k} closes Start{}", c.name)));
                                }
                                thing.components.push(open.take().unwrap());
                            }
                            (k, Some(c)) if k.starts_with("StartCTC") => {
                                return Err(f.err(format!("{k} inside Start{}", c.name)));
                            }
                            (_, Some(c)) => c.fields.push(Field::from(&f)),
                            (_, None) => thing.fields.push(Field::from(&f)),
                        }
                    }
                    tng.things.push(thing);
                }
                k => {
                    return Err(Error::Text {
                        line: s.line,
                        reason: format!("unknown top-level key {k:?}"),
                    });
                }
            }
        }
        Ok(tng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Version 2;\r\n\r\nXXXSectionStart NULL;\r\n\r\n\
        NewThing Building;\r\nPlayer 4;\r\nUID 18446741874686296296;\r\n\
        DefinitionType \"BUILDING_TOWN_WAREHOUSE_02\";\r\nScriptName NULL;\r\n\
        StartCTCPhysicsStandard;\r\nPositionX 7.5;\r\nPositionY 31.25;\r\nPositionZ 6.99;\r\n\
        RHSetForwardX -0.4;\r\nRHSetForwardY 0.9;\r\nRHSetForwardZ 0.0;\r\n\
        RHSetUpX 0.0;\r\nRHSetUpY 0.0;\r\nRHSetUpZ 1.0;\r\nEndCTCPhysicsStandard;\r\n\
        StartCTCEditor;\r\nEndCTCEditor;\r\nHealth 1.0;\r\nEndThing;\r\n\r\n\
        NewThing Holy Site;\r\nDefinitionType \"HOLY_SITE\";\r\nScriptName HS_1;\r\nEndThing;\r\n\
        XXXSectionEnd;\r\n";

    #[test]
    fn parses_things_and_components() {
        let tng = Tng::parse(SAMPLE).unwrap();
        assert_eq!(tng.version, 2);
        assert_eq!(tng.things.len(), 2);
        let b = &tng.things[0];
        assert_eq!((b.kind.as_str(), b.section.as_str()), ("Building", "NULL"));
        assert_eq!(b.definition(), Some("BUILDING_TOWN_WAREHOUSE_02"));
        assert_eq!(b.uid(), Some(18446741874686296296));
        assert_eq!(b.script_name(), None);
        assert_eq!(b.position(), Some([7.5, 31.25, 6.99]));
        assert_eq!(b.forward(), Some([-0.4, 0.9, 0.0]));
        assert_eq!(b.field("Health"), Some("1.0"));
        assert_eq!(b.components.len(), 2);
        let h = &tng.things[1];
        assert_eq!(
            (h.kind.as_str(), h.script_name()),
            ("Holy Site", Some("HS_1"))
        );
        assert_eq!(h.position(), None);
    }

    #[test]
    fn rejects_unbalanced_components() {
        let text = "NewThing Object;\nStartCTCEditor;\nEndCTCPhysicsStandard;\nEndThing;\n";
        assert!(matches!(Tng::parse(text), Err(Error::Text { line: 3, .. })));
    }
}
