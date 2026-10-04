//! Reopening where the app was, after it restarts itself for an update: the
//! window's place and state are passed to the new copy as `--restore`.

/// How the window was: its restored bounds (logical pixels) and state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Restore {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub state: WindowState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Normal,
    Maximized,
    Minimized,
}

impl Restore {
    /// The argument that carries it: `x,y,width,height,state`.
    pub fn to_arg(&self) -> String {
        let state = match self.state {
            WindowState::Normal => "normal",
            WindowState::Maximized => "maximized",
            WindowState::Minimized => "minimized",
        };
        format!("{},{},{},{},{state}", self.x, self.y, self.width, self.height)
    }

    pub fn from_arg(arg: &str) -> Option<Self> {
        let parts: Vec<&str> = arg.split(',').collect();
        let [x, y, width, height, state] = parts.as_slice() else { return None };
        let state = match *state {
            "normal" => WindowState::Normal,
            "maximized" => WindowState::Maximized,
            "minimized" => WindowState::Minimized,
            _ => return None,
        };
        let number = |s: &str| s.parse::<f32>().ok().filter(|n| n.is_finite());
        let restore = Self { x: number(x)?, y: number(y)?, width: number(width)?, height: number(height)?, state };
        (restore.width > 0. && restore.height > 0.).then_some(restore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let restore = Restore { x: -1200.5, y: 40., width: 1208., height: 1034., state: WindowState::Maximized };
        assert_eq!(Restore::from_arg(&restore.to_arg()), Some(restore));
    }

    #[test]
    fn rejects_nonsense() {
        assert_eq!(Restore::from_arg("1,2,3"), None);
        assert_eq!(Restore::from_arg("1,2,0,4,normal"), None);
        assert_eq!(Restore::from_arg("1,2,3,4,sideways"), None);
    }
}
