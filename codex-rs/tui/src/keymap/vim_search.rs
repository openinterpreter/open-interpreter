//! Search bindings shared by normal and operator-pending input. Existing explicit Vim
//! bindings win over new defaults; explicit search conflicts remain configuration errors.

use super::bindings::configured_binding_for_action;
use super::*;

#[derive(Clone, Debug)]
pub(crate) struct VimSearchKeymap {
    pub(crate) forward: Vec<KeyBinding>,
    pub(crate) backward: Vec<KeyBinding>,
    pub(crate) next: Vec<KeyBinding>,
    pub(crate) previous: Vec<KeyBinding>,
}

impl Default for VimSearchKeymap {
    fn default() -> Self {
        use key_hint::plain;
        use key_hint::shift;
        Self {
            forward: vec![plain(KeyCode::Char('/'))],
            backward: vec![plain(KeyCode::Char('?')), shift(KeyCode::Char('?'))],
            next: vec![plain(KeyCode::Char('n'))],
            previous: vec![plain(KeyCode::Char('N')), shift(KeyCode::Char('n'))],
        }
    }
}

impl RuntimeKeymap {
    pub(super) fn configure_vim_search(&mut self, config: &TuiKeymap) -> Result<(), String> {
        let others: Vec<_> = runtime_action_bindings(self)
            .filter(|binding| {
                matches!(
                    binding.id.context,
                    KeymapContext::VimNormal | KeymapContext::VimOperator
                )
            })
            .collect();
        let mut search = self.vim_search.clone();
        let config_search = &config.vim_search;
        let mut actions = [
            ("forward", &config_search.forward, &mut search.forward),
            ("backward", &config_search.backward, &mut search.backward),
            ("next", &config_search.next, &mut search.next),
            ("previous", &config_search.previous, &mut search.previous),
        ];
        for (action, configured, bindings) in &mut actions {
            let path = format!("tui.keymap.vim_search.{action}");
            if configured.is_none() {
                bindings.retain(|key| {
                    !others.iter().any(|other| {
                        other.bindings.contains(key)
                            && configured_binding_for_action(config, other.id)
                                .is_some_and(Option::is_some)
                    }) && !self.chords.bindings.iter().any(|chord| {
                        chord.action.context.overlaps(KeymapContext::VimSearch)
                            && chord.chord.prefix.parts() == key.parts()
                    })
                });
            }
            if let Some(other) = others
                .iter()
                .find(|other| other.bindings.iter().any(|key| bindings.contains(key)))
            {
                return Err(format!(
                    "Conflicting `{path}` and `{}` bindings",
                    other.id.config_path()
                ));
            }
        }
        validate_unique(
            "vim_search",
            actions.map(|(action, _, bindings)| (action, bindings.as_slice())),
        )?;
        self.vim_search = search;
        Ok(())
    }
}
