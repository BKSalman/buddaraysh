use smithay::{
    delegate_keyboard_shortcuts_inhibit,
    wayland::keyboard_shortcuts_inhibit::{
        KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
    },
};

use crate::{Backend, Buddaraysh};

impl<BackendData: Backend> KeyboardShortcutsInhibitHandler for Buddaraysh<BackendData> {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.keyboard_shortcuts_inhibit_state
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        // TODO: decide when to inhibit
        inhibitor.activate();
    }
}
delegate_keyboard_shortcuts_inhibit!(@<BackendData: Backend + 'static> Buddaraysh<BackendData>);
