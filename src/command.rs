use crate::{
    document::{Board, Card, Kind},
    geometry::Shape,
    scene::Scene,
    templates, Canvas,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use eframe::egui::vec2;
use serde_json::{json, Value};

pub const USAGE: &[&str] = &[
    "help | history",
    "board list | show [BOARD] | new [TEMPLATE] | open BOARD",
    "board rename BOARD TITLE | close [BOARD] | delete BOARD",
    "object list | show ID | select [ID]",
    "object add sticky|component|text|frame X Y TEXT",
    "object move ID X Y | resize ID WIDTH HEIGHT | text ID TEXT",
    "object shape ID rectangle|ellipse|diamond | delete ID",
    "connector add FROM TO [LABEL]",
    "template list | insert TEMPLATE",
    "story axes FRAME | add FRAME column|lane TITLE",
    "story move FRAME column|lane INDEX up|down",
    "view status | home | board | cancel | fit | pan DX DY | zoom FACTOR [X Y]",
    "editor name [NAME]",
    "share status | invite [BOARD] | join LINK | stop | install",
    "save | quit",
];
const TEMPLATES: [&str; 3] = ["brainstorming", "architecture", "story-map"];

pub fn format(args: &[&str]) -> String {
    args.iter()
        .map(|arg| {
            if !arg.is_empty()
                && arg
                    .chars()
                    .all(|c| c.is_alphanumeric() || "_:./-".contains(c))
            {
                (*arg).to_owned()
            } else {
                json!(arg).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn is_query(args: &[String]) -> bool {
    let group = args.first().map(String::as_str);
    let verb = args.get(1).map(String::as_str);
    matches!(
        (group, verb),
        (None | Some("help" | "--help" | "history"), _)
            | (Some("board" | "object"), Some("list" | "show"))
            | (Some("view" | "share"), Some("status"))
            | (Some("story"), Some("axes"))
            | (Some("template"), Some("list"))
    ) || (group == Some("editor") && args.len() == 2)
}

pub fn parse(line: &str) -> Result<Vec<String>> {
    let mut remaining = line.trim_start();
    let mut args = Vec::new();
    while !remaining.is_empty() {
        let (value, end) = match remaining.as_bytes()[0] {
            b'"' => {
                let mut stream =
                    serde_json::Deserializer::from_str(remaining).into_iter::<String>();
                let value = stream
                    .next()
                    .context("Missing quoted argument")?
                    .context("Invalid double-quoted argument")?;
                (value, stream.byte_offset())
            }
            b'\'' => {
                let end = remaining[1..].find('\'').context("Unclosed single quote")? + 1;
                (remaining[1..end].to_owned(), end + 1)
            }
            _ => {
                let end = remaining
                    .find(char::is_whitespace)
                    .unwrap_or(remaining.len());
                (remaining[..end].to_owned(), end)
            }
        };
        remaining = &remaining[end..];
        ensure!(
            remaining.is_empty() || remaining.starts_with(char::is_whitespace),
            "Separate arguments with spaces"
        );
        args.push(value);
        remaining = remaining.trim_start();
    }
    Ok(args)
}

fn number(value: &str) -> Result<f32> {
    let n: f32 = value
        .parse()
        .with_context(|| format!("Invalid number {value:?}"))?;
    ensure!(n.is_finite(), "Expected a finite number");
    Ok(n)
}

fn position(x: f32, y: f32) -> Result<()> {
    ensure!(
        x.abs() <= 1_000_000. && y.abs() <= 1_000_000.,
        "Object position outside the board limits"
    );
    Ok(())
}

fn template(value: &str) -> Result<usize> {
    TEMPLATES
        .iter()
        .zip(templates::NAMES)
        .position(|(key, title)| {
            value.eq_ignore_ascii_case(key) || value.eq_ignore_ascii_case(title)
        })
        .context("Unknown template. Use template list.")
}

pub fn template_key(index: usize) -> &'static str {
    TEMPLATES[index]
}

fn axis(value: &str) -> Result<bool> {
    match value {
        "column" => Ok(true),
        "lane" => Ok(false),
        _ => bail!("Expected column or lane"),
    }
}

fn direction(value: &str) -> Result<isize> {
    match value {
        "up" => Ok(-1),
        "down" => Ok(1),
        _ => bail!("Expected up or down"),
    }
}

pub enum Edit<'a> {
    Add(Kind, f32, f32, &'a str),
    Move(&'a str, f32, f32),
    Resize(&'a str, f32, f32),
    Text(&'a str, &'a str),
    Shape(&'a str, Shape),
    Connect(&'a str, &'a str, &'a str),
    Delete(&'a str),
    AxisAdd(&'a str, bool, &'a str),
    AxisMove(&'a str, bool, usize, isize),
}

fn card(board: &Board, id: &str) -> Result<Card> {
    board
        .cards()
        .into_iter()
        .find(|c| c.id == id)
        .context("Unknown card")
}

fn object(board: &Board, id: &str) -> Result<Value> {
    if let Ok(card) = card(board, id) {
        return Ok(json!(card));
    }
    let edge = board
        .connectors()
        .into_iter()
        .find(|edge| edge.id == id)
        .context("Unknown object")?;
    Ok(json!(edge))
}

fn axis_key(column: bool) -> &'static str {
    if column {
        "column"
    } else {
        "lane"
    }
}

impl Edit<'_> {
    pub fn line(&self) -> String {
        match *self {
            Self::Add(kind, x, y, label) => format(&[
                "object",
                "add",
                kind.key(),
                &x.to_string(),
                &y.to_string(),
                label,
            ]),
            Self::Move(id, x, y) => format(&["object", "move", id, &x.to_string(), &y.to_string()]),
            Self::Resize(id, w, h) => {
                format(&["object", "resize", id, &w.to_string(), &h.to_string()])
            }
            Self::Text(id, text) => format(&["object", "text", id, text]),
            Self::Shape(id, shape) => {
                format(&["object", "shape", id, &shape.name().to_ascii_lowercase()])
            }
            Self::Connect(from, to, label) => format(&["connector", "add", from, to, label]),
            Self::Delete(id) => format(&["object", "delete", id]),
            Self::AxisAdd(frame, column, title) => {
                format(&["story", "add", frame, axis_key(column), title])
            }
            Self::AxisMove(frame, column, index, delta) => format(&[
                "story",
                "move",
                frame,
                axis_key(column),
                &index.to_string(),
                if delta < 0 { "up" } else { "down" },
            ]),
        }
    }

    pub fn apply(&self, scene: &mut Scene, board: &mut Board) -> Result<String> {
        let id = match *self {
            Self::Add(kind, x, y, text) => {
                position(x, y)?;
                let id = board.add(kind, text, x, y)?;
                scene.select(Some(id.clone()));
                id
            }
            Self::Move(id, x, y) => {
                let mut c = card(board, id)?;
                ensure!(
                    !c.layout_header,
                    "Reorder this header through the story map"
                );
                position(x, y)?;
                (c.x, c.y) = (x, y);
                board.edit(&c)?;
                scene.finish_geometry(board, &c)?;
                id.into()
            }
            Self::Resize(id, w, h) => {
                let mut c = card(board, id)?;
                ensure!(
                    !c.layout_header && c.story.is_none(),
                    "Object size follows the story map"
                );
                (c.width, c.height) = (w, h);
                board.edit(&c)?;
                scene.finish_geometry(board, &c)?;
                id.into()
            }
            Self::Text(id, text) => {
                if id.starts_with("edge:") {
                    board.label_connector(id, text)?;
                } else {
                    let mut c = card(board, id)?;
                    c.text = text.into();
                    board.edit(&c)?;
                }
                id.into()
            }
            Self::Shape(id, shape) => {
                board.set_shape(id, shape)?;
                id.into()
            }
            Self::Connect(from, to, label) => {
                ensure!(label.len() <= 32_000, "Label is too long");
                let id = board.connect(from, to)?;
                if !label.is_empty() {
                    board.label_connector(&id, label)?;
                }
                scene.select(Some(id.clone()));
                id
            }
            Self::Delete(id) => {
                board.delete(id)?;
                if scene.selected.as_deref() == Some(id) {
                    scene.select(None);
                }
                if scene.connecting.as_deref() == Some(id) {
                    scene.connecting = None;
                }
                id.into()
            }
            Self::AxisAdd(id, column, title) => board.add_axis(id, column, title)?,
            Self::AxisMove(id, column, index, delta) => {
                board.move_axis(id, column, index, delta)?;
                id.into()
            }
        };
        Ok(id)
    }
}

impl Canvas {
    fn current_index(&self) -> Result<usize> {
        self.workspace
            .active
            .context("No active board. Use board open or board new.")
    }

    fn command_board_index(&self, id: &str) -> Result<usize> {
        self.board_index(id)
            .with_context(|| format!("Unknown board {id:?}. Use board list."))
    }

    fn current_board(&self) -> Result<&Board> {
        Ok(&self.workspace.entries[self.current_index()?].board)
    }

    pub fn edit_command(&mut self, edit: Edit<'_>) -> Result<Value> {
        let index = self.current_index()?;
        let id = edit.apply(&mut self.scene, &mut self.workspace.entries[index].board)?;
        self.result(Ok(()));
        Ok(json!({ "id": id }))
    }

    pub fn execute(&mut self, args: &[String], persistent: bool) -> Result<Value> {
        let args: Vec<_> = args.iter().map(String::as_str).collect();
        let args = args.as_slice();
        match args {
            [] | ["help"] | ["--help"] => Ok(json!(USAGE)),
            ["history"] => Ok(json!(self.terminal.history().collect::<Vec<_>>())),
            ["board", "list"] => {
                let boards: Vec<_> = self
                    .workspace
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| {
                        json!({
                            "id": entry.board.id(),
                            "title": entry.board.title(),
                            "open": entry.open,
                            "active": self.workspace.active == Some(index),
                        })
                    })
                    .collect();
                Ok(json!(boards))
            }
            ["board", "show", rest @ ..] if rest.len() <= 1 => {
                let index = self.target_index(rest)?;
                let entry = &self.workspace.entries[index];
                Ok(json!({
                    "id": entry.board.id(),
                    "title": entry.board.title(),
                    "open": entry.open,
                    "active": self.workspace.active == Some(index),
                    "cards": entry.board.cards(),
                    "connectors": entry.board.connectors(),
                }))
            }
            ["board", "new", rest @ ..] if rest.len() <= 1 => {
                let index = rest.first().map(|s| template(s)).transpose()?;
                self.create_board(index)?;
                Ok(json!({"id": self.current_board()?.id()}))
            }
            ["board", "open", id] => {
                self.open_board(self.command_board_index(id)?)?;
                Ok(json!({"id": id}))
            }
            ["board", "rename", id, title] => {
                self.rename_board(self.command_board_index(id)?, title)?;
                Ok(json!({"id": id, "title": title}))
            }
            ["board", "close", rest @ ..] if rest.len() <= 1 => {
                let index = self.target_index(rest)?;
                let id = self.workspace.entries[index].board.id();
                self.close_board(index)?;
                Ok(json!({"id": id}))
            }
            ["board", "delete", id] => {
                let path = self.delete_board(self.command_board_index(id)?)?;
                Ok(json!({"id": id, "trash": path}))
            }
            ["object", "list"] => {
                let board = self.current_board()?;
                Ok(json!({"cards": board.cards(), "connectors": board.connectors()}))
            }
            ["object", "show", id] => object(self.current_board()?, id),
            ["object", "select", rest @ ..] if rest.len() <= 1 => {
                let id = if let Some(id) = rest.first() {
                    object(self.current_board()?, id)?;
                    Some((*id).to_owned())
                } else {
                    None
                };
                self.scene.select(id.clone());
                Ok(json!({"selected": id}))
            }
            ["object", "add", kind, x, y, label] => {
                let kind = Kind::ALL
                    .into_iter()
                    .find(|candidate| candidate.key() == *kind)
                    .context("Unknown object kind")?;
                self.edit_command(Edit::Add(kind, number(x)?, number(y)?, label))
            }
            ["object", "move", id, x, y] => {
                self.edit_command(Edit::Move(id, number(x)?, number(y)?))
            }
            ["object", "resize", id, w, h] => {
                self.edit_command(Edit::Resize(id, number(w)?, number(h)?))
            }
            ["object", "text", id, label] => self.edit_command(Edit::Text(id, label)),
            ["object", "shape", id, name] => {
                let shape = Shape::ALL
                    .into_iter()
                    .find(|shape| shape.name().eq_ignore_ascii_case(name))
                    .context("Unknown shape")?;
                self.edit_command(Edit::Shape(id, shape))
            }
            ["object", "delete", id] => self.edit_command(Edit::Delete(id)),
            ["connector", "add", from, to, rest @ ..] if rest.len() <= 1 => self.edit_command(
                Edit::Connect(from, to, rest.first().copied().unwrap_or_default()),
            ),
            ["template", "list"] => {
                let templates: Vec<_> = TEMPLATES
                    .iter()
                    .zip(templates::NAMES)
                    .map(|(id, title)| json!({"id": id, "title": title}))
                    .collect();
                Ok(json!(templates))
            }
            ["template", "insert", name] => Ok(json!({"id": self.template(template(name)?)?})),
            ["story", "axes", frame] => {
                let axes = self
                    .current_board()?
                    .story_axes(frame)
                    .context("Not a story map")?;
                Ok(json!(axes))
            }
            ["story", "add", frame, kind, title] => {
                self.edit_command(Edit::AxisAdd(frame, axis(kind)?, title))
            }
            ["story", "move", frame, kind, index, delta] => {
                let index = index.parse().context("Expected a zero-based axis index")?;
                self.edit_command(Edit::AxisMove(frame, axis(kind)?, index, direction(delta)?))
            }
            ["view", "status"] => Ok(self.view_status()),
            ["view", "home"] => {
                self.go_home();
                Ok(self.view_status())
            }
            ["view", "board"] => {
                self.open_board(self.current_index()?)?;
                Ok(self.view_status())
            }
            ["view", "cancel"] => {
                self.cancel_interaction();
                Ok(self.view_status())
            }
            ["view", "fit"] => {
                self.fit_view()?;
                Ok(self.view_status())
            }
            ["view", "pan", x, y] => {
                self.scene.pan_by(vec2(number(x)?, number(y)?))?;
                Ok(self.view_status())
            }
            ["view", "zoom", factor, rest @ ..] if rest.is_empty() || rest.len() == 2 => {
                let factor = number(factor)?;
                ensure!(factor > 0., "Zoom factor must be positive");
                if let [x, y] = rest {
                    self.scene.zoom_at(vec2(number(x)?, number(y)?), factor)?;
                } else {
                    self.scene.zoom_center(factor)?;
                }
                Ok(self.view_status())
            }
            ["editor", "name", rest @ ..] if rest.len() <= 1 => {
                if let Some(name) = rest.first() {
                    self.set_editor_name(name)?;
                }
                Ok(json!({"name": self.display_name}))
            }
            ["share", "status"] => Ok(json!({
                "board": self.network.board, "joining": self.joining, "status": self.network_status,
                "error": self.network_error, "invitation": self.invitation,
                "peers": self.scene.remote.iter().map(|p| json!({"name": p.name, "cursor": p.cursor, "selected": p.selected})).collect::<Vec<_>>()
            })),
            ["share", "invite", rest @ ..] if rest.len() <= 1 => {
                ensure!(
                    persistent,
                    "Start Jotflock or jotflock --headless to keep a live session"
                );
                self.share(self.target_index(rest)?, false)?;
                Ok(json!({"pending": true}))
            }
            ["share", "join", link] => {
                ensure!(
                    persistent,
                    "Start Jotflock or jotflock --headless to keep a live session"
                );
                self.join_board(link)?;
                Ok(json!({"pending": true}))
            }
            ["share", "stop"] => {
                self.stop_sharing()?;
                Ok(json!({"sharing": false}))
            }
            ["share", "install"] => {
                desktop_install()?;
                Ok(json!({"installed": true}))
            }
            ["save"] => {
                self.flush()?;
                Ok(json!({"saved": true}))
            }
            ["quit"] => {
                self.flush()?;
                self.quit = true;
                Ok(json!({"closing": true}))
            }
            _ => Err(anyhow!("Unknown command or arguments. Use help.")),
        }
    }

    fn target_index(&self, rest: &[&str]) -> Result<usize> {
        match rest {
            [id] => self.command_board_index(id),
            _ => self.current_index(),
        }
    }

    fn view_status(&self) -> Value {
        json!({
            "home": self.home, "board": self.workspace.board().map(Board::id), "selected": self.scene.selected,
            "pan": [self.scene.pan.x, self.scene.pan.y], "zoom": self.scene.zoom,
            "editing": self.scene.editing, "connecting": self.scene.connecting
        })
    }
}

pub fn desktop_install() -> Result<()> {
    #[cfg(target_os = "linux")]
    return crate::desktop::install();
    #[cfg(not(target_os = "linux"))]
    bail!("Desktop invitation registration is available on Linux");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayed_commands_round_trip_with_text_and_paths() -> Result<()> {
        let args = [
            "object",
            "text",
            "card:abc",
            "It's π, \"quoted\"\nwith two lines\\",
        ];
        assert_eq!(parse(&format(&args))?, args);
        assert_eq!(
            parse("object text card:abc 'a b'")?,
            ["object", "text", "card:abc", "a b"]
        );
        assert_eq!(parse("object text card:abc \"\"")?[3], "");
        assert!(parse("object text card:abc \"unterminated").is_err());
        Ok(())
    }
}
