use egui::Color32;
use serde::{Serialize, Deserialize};
use zflow_runtime::ip::IPType;

pub type SVec<T> = smallvec::SmallVec<[T; 4]>;


slotmap::new_key_type! { pub struct InputId; }
slotmap::new_key_type! { pub struct OutputId; }


#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum AnyParameterId {
    Input(InputId),
    Output(OutputId),
}

impl AnyParameterId {
    pub fn assume_input(&self) -> InputId {
        match self {
            AnyParameterId::Input(input) => *input,
            AnyParameterId::Output(output) => panic!("{:?} is not an InputId", output),
        }
    }
    pub fn assume_output(&self) -> OutputId {
        match self {
            AnyParameterId::Output(output) => *output,
            AnyParameterId::Input(input) => panic!("{:?} is not an OutputId", input),
        }
    }
}

impl From<OutputId> for AnyParameterId {
    fn from(output: OutputId) -> Self {
        Self::Output(output)
    }
}

impl From<InputId> for AnyParameterId {
    fn from(input: InputId) -> Self {
        Self::Input(input)
    }
}

pub fn data_type_color(typ:&IPType) -> Color32 {
    match typ {
        &IPType::OpenBracket(_) => Color32::DARK_GRAY,
        &IPType::CloseBracket(_) => Color32::GRAY,
        &IPType::Data(_) => Color32::LIGHT_BLUE,
        &IPType::Buffer(_) => Color32::LIGHT_YELLOW,
        &IPType::All(_) => Color32::LIGHT_BLUE,
        &IPType::Unknown => Color32::WHITE,
    }
}