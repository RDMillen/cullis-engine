//! The command line.

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subject {
    Region(String),
    Mesh(String),
}

pub(crate) struct Args {
    pub(crate) subject: Subject,
    /// Screenshot camera override: target x, y, z, distance, yaw, pitch.
    pub(crate) camera: Option<[f32; 6]>,
    /// Draw the navigation overlay.
    pub(crate) nav: bool,
    /// Print ledges in the walk grid near the camera target (diagnostic).
    pub(crate) ledges: bool,
    /// Place the young Hero near the camera target (screenshots).
    pub(crate) walk: bool,
    /// Then walk him towards this point (x, y) before the screenshot.
    /// x, y, and the floor's height (NaN for any floor).
    pub(crate) walk_to: Option<[f32; 3]>,
    /// Tng sections (quest states) to show besides `NULL`; repeatable. Default:
    /// the region's starting state (`terrain::default_sections`).
    pub(crate) sections: Vec<String>,
    /// Pose the Hero with this animation (default: idle, or walk after
    /// `--walk-to`) at `anim_time` seconds.
    pub(crate) anim: Option<String>,
    pub(crate) anim_time: f32,
    /// Advance the creatures this many seconds before the screenshot.
    pub(crate) npc_time: f32,
    /// Leave out placed lights (lamps, fires), for comparing screenshots.
    pub(crate) no_lights: bool,
    /// Time of day, in hours, and the light's brightness.
    pub(crate) hour: f32,
    pub(crate) brightness: f32,
    /// The most light a surface may get (see `lighting::HIGHLIGHT_LIMIT`).
    pub(crate) highlights: f32,
    /// After walking (`--walk-to`), use the nearest door, then walk again.
    pub(crate) use_door: bool,
    /// After walking, talk to the nearest creature this many times, printing
    /// what it says.
    pub(crate) talk: u32,
    /// Start the Prologue quest with Theresa's present bought (for testing the
    /// ending).
    pub(crate) with_present: bool,
    /// Make the creature with this role start a happy conversation gesture
    /// before `--npc-time` runs (for checking gestures in screenshots).
    pub(crate) gesture: Option<String>,
    /// Hold the mouth of the creature with this role in a shape
    /// (`NOVI_LiveFather:AH`), for checking lip sync in screenshots.
    pub(crate) mouth: Option<(String, String)>,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) size: [u32; 2],
}

pub(crate) fn parse_args() -> Result<Args, Error> {
    let mut args = Args {
        subject: Subject::Region("StartOakVale".into()),
        screenshot: None,
        camera: None,
        nav: false,
        ledges: false,
        walk: false,
        walk_to: None,
        sections: Vec::new(),
        anim: None,
        anim_time: 0.0,
        npc_time: 0.0,
        no_lights: false,
        hour: DEFAULT_HOUR,
        brightness: lighting::BRIGHTNESS,
        highlights: lighting::HIGHLIGHT_LIMIT,
        use_door: false,
        talk: 0,
        with_present: false,
        gesture: None,
        mouth: None,
        size: [1280, 800],
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--region" => args.subject = Subject::Region(value()?),
            "--mesh" => args.subject = Subject::Mesh(value()?),
            "--screenshot" => args.screenshot = Some(value()?.into()),
            "--nav" => args.nav = true,
            "--ledges" => args.ledges = true,
            "--walk" => args.walk = true,
            "--section" => args.sections.push(value()?),
            "--anim" => {
                args.anim = Some(value()?);
                args.walk = true;
            }
            "--anim-time" => args.anim_time = value()?.parse()?,
            "--npc-time" => args.npc_time = value()?.parse()?,
            "--hour" => args.hour = value()?.parse()?,
            "--no-lights" => args.no_lights = true,
            "--brightness" => args.brightness = value()?.parse()?,
            "--highlights" => args.highlights = value()?.parse()?,
            "--use" => args.use_door = true,
            "--talk" => args.talk = value()?.parse()?,
            "--with-present" => args.with_present = true,
            "--gesture" => args.gesture = Some(value()?),
            "--mouth" => {
                let v = value()?;
                let (role, shape) = v.split_once(':').ok_or("--mouth takes ROLE:SHAPE")?;
                args.mouth = Some((role.to_owned(), shape.to_owned()));
            }
            "--walk-to" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()?;
                // x,y, or x,y,z for a floor among stacked ones.
                args.walk_to = match *v.as_slice() {
                    [x, y] => Some([x, y, f32::NAN]),
                    [x, y, z] => Some([x, y, z]),
                    _ => return Err("--walk-to takes x,y or x,y,z".into()),
                };
                args.walk = true;
            }
            "--camera" => {
                let v: Vec<f32> = value()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()?;
                args.camera = Some(
                    v.try_into()
                        .map_err(|_| "--camera takes x,y,z,distance,yaw,pitch")?,
                );
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size takes WxH")?;
                args.size = [w.parse()?, h.parse()?];
            }
            _ => return Err(format!("unknown argument {arg:?}").into()),
        }
    }
    Ok(args)
}
