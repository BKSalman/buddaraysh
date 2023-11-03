use smithay::reexports::wayland_server::DisplayHandle;

use crate::state::State;

pub struct Data {
    pub display_handle: DisplayHandle,
    pub state: State,
}
