use crate::events::AppEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    File,
    Edit,
    Navigation,
    Request,
    Collection,
    Environment,
    Settings,
    Help,
    System,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub category: CommandCategory,
    pub execute: fn() -> Vec<AppEvent>,
}

impl Command {
    pub fn matches(&self, input: &str) -> bool {
        let input_lower = input.to_lowercase();
        if input_lower.is_empty() {
            return true;
        }
        if self.name.to_lowercase().starts_with(&input_lower) {
            return true;
        }
        for alias in self.aliases {
            if alias.to_lowercase().starts_with(&input_lower) {
                return true;
            }
        }
        false
    }

    pub fn fuzzy_score(&self, input: &str) -> Option<u32> {
        if input.is_empty() {
            return None;
        }
        let input_lower = input.to_lowercase();
        let name_lower = self.name.to_lowercase();

        if name_lower.starts_with(&input_lower) {
            return Some(input_lower.len() as u32 * 100);
        }
        for alias in self.aliases {
            if alias.to_lowercase().starts_with(&input_lower) {
                return Some(input_lower.len() as u32 * 90);
            }
        }

        // Fuzzy: all input chars must appear in order
        let mut name_iter = name_lower.chars();
        let mut matched = 0u32;
        let mut all_found = true;
        for c in input_lower.chars() {
            let mut found = false;
            while let Some(nc) = name_iter.next() {
                matched += 1;
                if nc == c {
                    found = true;
                    break;
                }
            }
            if !found {
                all_found = false;
                break;
            }
        }
        if all_found && matched > 0 {
            Some(matched)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

impl CommandRegistry {
    pub fn register(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn all(&self) -> &[Command] {
        &self.commands
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Command> {
        let name_lower = name.to_lowercase();
        self.commands.iter().find(|c| {
            c.name.to_lowercase() == name_lower
                || c.aliases.iter().any(|a| a.to_lowercase() == name_lower)
        })
    }

    pub fn search(&self, query: &str) -> Vec<&Command> {
        if query.is_empty() {
            return self.commands.iter().collect();
        }

        let mut scored: Vec<(u32, &Command)> = self
            .commands
            .iter()
            .filter_map(|c| c.fuzzy_score(query).map(|s| (s, c)))
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, c)| c).collect()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::default();

        registry.register(Command {
            name: "send",
            aliases: &["run"],
            description: "Execute current request",
            category: CommandCategory::Request,
            execute: || vec![AppEvent::ExecuteRequest],
        });

        registry.register(Command {
            name: "save",
            aliases: &["w", "write"],
            description: "Save current request",
            category: CommandCategory::File,
            execute: || vec![AppEvent::SaveState],
        });

        registry.register(Command {
            name: "quit",
            aliases: &["q", "close"],
            description: "Close current tab or quit if last tab",
            category: CommandCategory::System,
            execute: || vec![AppEvent::Quit],
        });

        registry.register(Command {
            name: "quit-force",
            aliases: &["q!"],
            description: "Force quit without saving",
            category: CommandCategory::System,
            execute: || vec![AppEvent::Quit],
        });

        registry.register(Command {
            name: "save-and-close",
            aliases: &["wq", "x"],
            description: "Save current request and close tab",
            category: CommandCategory::File,
            execute: || vec![AppEvent::SaveState, AppEvent::Quit],
        });

        registry.register(Command {
            name: "new",
            aliases: &["tabnew"],
            description: "Create a new request tab",
            category: CommandCategory::Navigation,
            execute: || {
                vec![AppEvent::TabOpened {
                    id: "new".to_string(),
                }]
            },
        });

        registry.register(Command {
            name: "edit",
            aliases: &["e"],
            description: "Open a request for editing",
            category: CommandCategory::Navigation,
            execute: || vec![AppEvent::SearchActivated],
        });

        registry.register(Command {
            name: "collections",
            aliases: &["col"],
            description: "Open collections sidebar",
            category: CommandCategory::Collection,
            execute: || vec![AppEvent::PaneChanged(crate::state::ActivePane::Sidebar)],
        });

        registry.register(Command {
            name: "history",
            aliases: &["hist"],
            description: "Show request history",
            category: CommandCategory::Navigation,
            execute: || vec![AppEvent::PaneChanged(crate::state::ActivePane::Logs)],
        });

        registry.register(Command {
            name: "environment",
            aliases: &["env"],
            description: "Switch active environment",
            category: CommandCategory::Environment,
            execute: || vec![AppEvent::SettingsOpened],
        });

        registry.register(Command {
            name: "import",
            aliases: &["i"],
            description: "Import a collection (Postman, curl, etc)",
            category: CommandCategory::Collection,
            execute: || {
                vec![AppEvent::ImportStarted {
                    source: "manual".to_string(),
                }]
            },
        });

        registry.register(Command {
            name: "export",
            aliases: &["export-collection"],
            description: "Export current collection as Postman v2.1",
            category: CommandCategory::Collection,
            execute: || vec![AppEvent::SettingsOpened],
        });

        registry.register(Command {
            name: "theme",
            aliases: &[],
            description: "Change theme (usage: :theme <name>)",
            category: CommandCategory::Settings,
            execute: || vec![],
        });

        registry.register(Command {
            name: "set",
            aliases: &[],
            description: "Set configuration option (usage: :set key=value)",
            category: CommandCategory::Settings,
            execute: || vec![AppEvent::SettingsOpened],
        });

        registry.register(Command {
            name: "help",
            aliases: &["h", "?"],
            description: "Show help screen",
            category: CommandCategory::Help,
            execute: || vec![AppEvent::SettingsOpened],
        });

        registry.register(Command {
            name: "fullscreen",
            aliases: &["toggle-fullscreen"],
            description: "Toggle fullscreen for current pane",
            category: CommandCategory::Navigation,
            execute: || vec![AppEvent::MaximizePaneHeight],
        });

        registry.register(Command {
            name: "sidebar",
            aliases: &["toggle-sidebar"],
            description: "Toggle sidebar visibility",
            category: CommandCategory::Navigation,
            execute: || vec![AppEvent::PaneChanged(crate::state::ActivePane::Sidebar)],
        });

        registry.register(Command {
            name: "search",
            aliases: &["find", "/"],
            description: "Search within current view",
            category: CommandCategory::Navigation,
            execute: || vec![AppEvent::SearchActivated],
        });

        registry.register(Command {
            name: "clear",
            aliases: &["clear-logs"],
            description: "Clear the activity log",
            category: CommandCategory::System,
            execute: || vec![AppEvent::ClearLogs],
        });

        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_new() {
        let cmd = Command {
            name: "send",
            aliases: &["run"],
            description: "Send request",
            category: CommandCategory::Request,
            execute: || vec![AppEvent::ExecuteRequest],
        };
        assert_eq!(cmd.name, "send");
        assert_eq!(cmd.aliases[0], "run");
    }

    #[test]
    fn test_command_matches_exact() {
        let cmd = Command {
            name: "send",
            aliases: &["run"],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![],
        };
        assert!(cmd.matches("send"));
        assert!(cmd.matches("run"));
        assert!(!cmd.matches("xend"));
    }

    #[test]
    fn test_command_matches_prefix() {
        let cmd = Command {
            name: "send",
            aliases: &["run"],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![],
        };
        assert!(cmd.matches("se"));
        assert!(cmd.matches("ru"));
        assert!(cmd.matches("s"));
        assert!(!cmd.matches("x"));
    }

    #[test]
    fn test_command_matches_case_insensitive() {
        let cmd = Command {
            name: "Send",
            aliases: &["Run"],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![],
        };
        assert!(cmd.matches("send"));
        assert!(cmd.matches("SEND"));
        assert!(cmd.matches("SeNd"));
    }

    #[test]
    fn test_command_fuzzy_score_prefix_best() {
        let cmd = Command {
            name: "send",
            aliases: &["run"],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![],
        };
        let prefix = cmd.fuzzy_score("se");
        let fuzzy = cmd.fuzzy_score("snd");
        assert!(prefix > fuzzy);
    }

    #[test]
    fn test_command_registry_new() {
        let registry = CommandRegistry::default();
        assert!(registry.all().is_empty());
    }

    #[test]
    fn test_command_registry_register() {
        let mut registry = CommandRegistry::default();
        registry.register(Command {
            name: "send",
            aliases: &["run"],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![],
        });
        assert_eq!(registry.all().len(), 1);
    }

    #[test]
    fn test_command_registry_find_by_name() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.find_by_name("send").is_some());
        assert!(registry.find_by_name("run").is_some());
        assert!(registry.find_by_name("save").is_some());
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_command_registry_search_empty() {
        let registry = CommandRegistry::with_defaults();
        let results = registry.search("");
        assert_eq!(results.len(), registry.all().len());
    }

    #[test]
    fn test_command_registry_search_prefix() {
        let registry = CommandRegistry::with_defaults();
        let results = registry.search("se");
        assert!(!results.is_empty());
        assert!(results.iter().any(|c| c.name == "send"));
    }

    #[test]
    fn test_command_registry_search_alias() {
        let registry = CommandRegistry::with_defaults();
        let results = registry.search("run");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_command_registry_default_has_commands() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.all().len() >= 15);
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.find_by_name("SEND").is_some());
        assert!(registry.find_by_name("Save").is_some());
    }

    #[test]
    fn test_command_category_variants() {
        let categories = vec![
            CommandCategory::File,
            CommandCategory::Edit,
            CommandCategory::Navigation,
            CommandCategory::Request,
            CommandCategory::Collection,
            CommandCategory::Environment,
            CommandCategory::Settings,
            CommandCategory::Help,
            CommandCategory::System,
        ];
        assert_eq!(categories.len(), 9);
    }

    #[test]
    fn test_send_command_produces_execute_event() {
        let cmd = Command {
            name: "send",
            aliases: &[],
            description: "",
            category: CommandCategory::Request,
            execute: || vec![AppEvent::ExecuteRequest],
        };
        let events = (cmd.execute)();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AppEvent::ExecuteRequest));
    }

    #[test]
    fn test_save_command_produces_save_event() {
        let cmd = Command {
            name: "save",
            aliases: &["w"],
            description: "",
            category: CommandCategory::File,
            execute: || vec![AppEvent::SaveState],
        };
        let events = (cmd.execute)();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AppEvent::SaveState));
    }

    #[test]
    fn test_wq_produces_two_events() {
        let cmd = Command {
            name: "save-and-close",
            aliases: &["wq"],
            description: "",
            category: CommandCategory::File,
            execute: || vec![AppEvent::SaveState, AppEvent::Quit],
        };
        let events = (cmd.execute)();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_fuzzy_score_empty_input() {
        let cmd = Command {
            name: "anything",
            aliases: &[],
            description: "",
            category: CommandCategory::System,
            execute: || vec![],
        };
        assert!(cmd.fuzzy_score("").is_none());
    }
}
