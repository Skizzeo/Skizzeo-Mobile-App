//! 03-workflow: verbindliche Übergangstabelle des Zeichenmodus.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowState {
    View,
    Draw,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowEvent {
    Start,
    Stop,
    RequestClose,
    Resume,
}

impl WorkflowState {
    fn name(self) -> &'static str {
        match self {
            WorkflowState::View => "VIEW",
            WorkflowState::Draw => "DRAW",
            WorkflowState::Close => "CLOSE",
        }
    }
}

impl WorkflowEvent {
    fn name(self) -> &'static str {
        match self {
            WorkflowEvent::Start => "START",
            WorkflowEvent::Stop => "STOP",
            WorkflowEvent::RequestClose => "REQUEST_CLOSE",
            WorkflowEvent::Resume => "RESUME",
        }
    }
}

pub fn transition(state: WorkflowState, event: WorkflowEvent) -> Result<WorkflowState, String> {
    use WorkflowEvent as E;
    use WorkflowState as S;
    let next = match (state, event) {
        (S::View, E::Start) => Some(S::Draw),
        (S::View, E::Stop) => Some(S::View),
        (S::View, E::Resume) => Some(S::View),
        (S::Draw, E::Start) => Some(S::Draw),
        (S::Draw, E::Stop) => Some(S::View),
        (S::Draw, E::RequestClose) => Some(S::Close),
        (S::Draw, E::Resume) => Some(S::Draw),
        (S::Close, E::Start) => Some(S::Draw),
        (S::Close, E::Stop) => Some(S::View),
        (S::Close, E::RequestClose) => Some(S::Close),
        (S::Close, E::Resume) => Some(S::Draw),
        (S::View, E::RequestClose) => None,
    };
    next.ok_or_else(|| {
        format!(
            "Ungültiger Zustandswechsel: {} / {}",
            state.name(),
            event.name()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(transition(WorkflowState::View, WorkflowEvent::Start), Ok(WorkflowState::Draw));
        assert_eq!(transition(WorkflowState::Close, WorkflowEvent::Resume), Ok(WorkflowState::Draw));
        assert!(transition(WorkflowState::View, WorkflowEvent::RequestClose).is_err());
    }
}
