// Author: Julian Bolivar
// Version: 1.0.0
// Date: 2026-04-05

use crate::error::{MagiError, ProviderError};
use crate::provider::{CompletionConfig, LlmProvider};
use crate::schema::{AgentName, Mode};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use crate::prompts;

/// An autonomous MAGI agent with its own identity, system prompt, and LLM provider.
///
/// Each agent combines an [`AgentName`] identity, a [`Mode`]-specific system prompt,
/// and an [`LlmProvider`] backend. The agent delegates LLM communication to its
/// provider via [`execute`](Agent::execute).
pub struct Agent {
    name: AgentName,
    mode: Mode,
    system_prompt: String,
    provider: Arc<dyn LlmProvider>,
}

impl Agent {
    /// Creates an agent with an auto-generated system prompt for the given name and mode.
    ///
    /// The prompt is selected from compiled-in markdown files via `include_str!`.
    ///
    /// # Parameters
    /// - `name`: Which MAGI agent (Melchior, Balthasar, Caspar).
    /// - `mode`: Analysis mode (CodeReview, Design, Analysis).
    /// - `provider`: The LLM backend for this agent.
    pub fn new(name: AgentName, mode: Mode, provider: Arc<dyn LlmProvider>) -> Self {
        let prompt = match name {
            AgentName::Melchior => prompts::melchior::prompt_for_mode(&mode),
            AgentName::Balthasar => prompts::balthasar::prompt_for_mode(&mode),
            AgentName::Caspar => prompts::caspar::prompt_for_mode(&mode),
        };
        Self {
            name,
            mode,
            system_prompt: prompt.to_string(),
            provider,
        }
    }

    /// Creates an agent with a custom system prompt, bypassing the compiled-in defaults.
    ///
    /// # Parameters
    /// - `name`: Which MAGI agent.
    /// - `mode`: Analysis mode.
    /// - `provider`: The LLM backend.
    /// - `prompt`: Custom system prompt string.
    pub fn with_custom_prompt(
        name: AgentName,
        mode: Mode,
        provider: Arc<dyn LlmProvider>,
        prompt: String,
    ) -> Self {
        Self {
            name,
            mode,
            system_prompt: prompt,
            provider,
        }
    }

    /// Creates an agent by loading the system prompt from a filesystem path.
    ///
    /// Returns [`MagiError::Io`] if the file cannot be read.
    ///
    /// # Parameters
    /// - `name`: Which MAGI agent.
    /// - `mode`: Analysis mode.
    /// - `provider`: The LLM backend.
    /// - `path`: Path to the prompt file.
    ///
    /// # Errors
    /// Returns `MagiError::Io` if the file does not exist or cannot be read.
    pub fn from_file(
        name: AgentName,
        mode: Mode,
        provider: Arc<dyn LlmProvider>,
        path: &Path,
    ) -> Result<Self, MagiError> {
        let prompt = std::fs::read_to_string(path)?;
        Ok(Self {
            name,
            mode,
            system_prompt: prompt,
            provider,
        })
    }

    /// Executes the agent by sending the user prompt to the LLM provider.
    ///
    /// Delegates to [`LlmProvider::complete`] with this agent's system prompt.
    /// Returns the raw LLM response string — parsing is the orchestrator's responsibility.
    ///
    /// # Parameters
    /// - `user_prompt`: The user's input content.
    /// - `config`: Completion parameters (max_tokens, temperature).
    ///
    /// # Errors
    /// Returns `ProviderError` on LLM communication failure.
    pub async fn execute(
        &self,
        user_prompt: &str,
        config: &CompletionConfig,
    ) -> Result<String, ProviderError> {
        self.provider
            .complete(&self.system_prompt, user_prompt, config)
            .await
    }

    /// Returns the agent's name.
    pub fn name(&self) -> AgentName {
        self.name
    }

    /// Returns the agent's analysis mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Returns the agent's system prompt.
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Returns the provider's name (e.g., "claude", "openai").
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Returns the provider's model identifier.
    pub fn provider_model(&self) -> &str {
        self.provider.model()
    }

    /// Returns the agent's display name (e.g., "Melchior").
    pub fn display_name(&self) -> &str {
        self.name.display_name()
    }

    /// Returns the agent's analytical role title (e.g., "Scientist").
    pub fn title(&self) -> &str {
        self.name.title()
    }
}

/// Factory for creating sets of three MAGI agents with provider and prompt overrides.
///
/// Supports a default provider shared by all agents, per-agent provider overrides,
/// and custom prompt overrides. Always creates agents in order:
/// `[Melchior, Balthasar, Caspar]`.
pub struct AgentFactory {
    default_provider: Arc<dyn LlmProvider>,
    agent_providers: BTreeMap<AgentName, Arc<dyn LlmProvider>>,
    custom_prompts: BTreeMap<AgentName, String>,
}

impl AgentFactory {
    /// Creates a factory with a default provider shared by all three agents.
    ///
    /// # Parameters
    /// - `default_provider`: The LLM provider used for agents without a specific override.
    pub fn new(default_provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            default_provider,
            agent_providers: BTreeMap::new(),
            custom_prompts: BTreeMap::new(),
        }
    }

    /// Registers a provider override for a specific agent.
    ///
    /// # Parameters
    /// - `name`: Which agent to override.
    /// - `provider`: The provider to use for this agent.
    pub fn with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self {
        self.agent_providers.insert(name, provider);
        self
    }

    /// Registers a custom prompt override for a specific agent.
    ///
    /// # Parameters
    /// - `name`: Which agent to override.
    /// - `prompt`: The custom system prompt.
    pub fn with_custom_prompt(mut self, name: AgentName, prompt: String) -> Self {
        self.custom_prompts.insert(name, prompt);
        self
    }

    /// Loads custom prompts from a directory of markdown files.
    ///
    /// Expected filenames: `{agent}_{mode}.md` (e.g., `melchior_code_review.md`).
    /// Only loads files that exist; missing files use the default compiled-in prompts.
    /// Returns [`MagiError::Io`] if the directory itself does not exist.
    ///
    /// # Errors
    /// Returns `MagiError::Io` if the directory does not exist or cannot be read.
    pub fn from_directory(mut self, dir: &Path) -> Result<Self, MagiError> {
        // Verify the directory exists
        std::fs::read_dir(dir)?;

        let agents = ["melchior", "balthasar", "caspar"];
        let modes = ["code_review", "design", "analysis"];

        for agent_str in &agents {
            for mode_str in &modes {
                let filename = format!("{agent_str}_{mode_str}.md");
                let path = dir.join(&filename);
                if path.exists() {
                    let content = std::fs::read_to_string(&path)?;
                    let agent_name = match *agent_str {
                        "melchior" => AgentName::Melchior,
                        "balthasar" => AgentName::Balthasar,
                        "caspar" => AgentName::Caspar,
                        _ => unreachable!(),
                    };
                    self.custom_prompts.insert(agent_name, content);
                }
            }
        }

        Ok(self)
    }

    /// Creates exactly three agents for the given mode.
    ///
    /// Returns agents in fixed order: `[Melchior, Balthasar, Caspar]`.
    /// Each agent uses its specific provider override or the default provider,
    /// and its custom prompt override or the compiled-in default prompt.
    ///
    /// # Parameters
    /// - `mode`: The analysis mode for all three agents.
    pub fn create_agents(&self, mode: Mode) -> Vec<Agent> {
        let names = [AgentName::Melchior, AgentName::Balthasar, AgentName::Caspar];

        names
            .iter()
            .map(|&name| {
                let provider = self
                    .agent_providers
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| self.default_provider.clone());

                if let Some(prompt) = self.custom_prompts.get(&name) {
                    Agent::with_custom_prompt(name, mode, provider, prompt.clone())
                } else {
                    Agent::new(name, mode, provider)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock LlmProvider that tracks call count and returns a configurable response.
    struct MockProvider {
        name: String,
        model: String,
        response: String,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(name: &str, model: &str, response: &str) -> Self {
            Self {
                name: name.to_string(),
                model: model.to_string(),
                response: response.to_string(),
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _config: &CompletionConfig,
        ) -> Result<String, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.response.clone())
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn model(&self) -> &str {
            &self.model
        }
    }

    // -- BDD Scenario 26: agents with different providers --

    /// Each agent uses its own provider (verify mock receives exactly 1 call).
    #[tokio::test]
    async fn test_each_agent_uses_its_own_provider() {
        let p1 = Arc::new(MockProvider::new("p1", "m1", "r1"));
        let p2 = Arc::new(MockProvider::new("p2", "m2", "r2"));
        let p3 = Arc::new(MockProvider::new("p3", "m3", "r3"));

        let factory = AgentFactory::new(p1.clone() as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Melchior, p1.clone() as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Balthasar, p2.clone() as Arc<dyn LlmProvider>)
            .with_provider(AgentName::Caspar, p3.clone() as Arc<dyn LlmProvider>);

        let agents = factory.create_agents(Mode::CodeReview);
        let config = CompletionConfig::default();

        for agent in &agents {
            let _ = agent.execute("test input", &config).await;
        }

        assert_eq!(p1.calls(), 1, "p1 should receive exactly 1 call");
        assert_eq!(p2.calls(), 1, "p2 should receive exactly 1 call");
        assert_eq!(p3.calls(), 1, "p3 should receive exactly 1 call");
    }

    // -- BDD Scenario 27: factory with default and override --

    /// Factory uses default provider for unoverridden agents, override for Caspar.
    #[tokio::test]
    async fn test_factory_default_and_override_providers() {
        let default = Arc::new(MockProvider::new("default", "m1", "r1"));
        let caspar_override = Arc::new(MockProvider::new("caspar-special", "m2", "r2"));

        let factory = AgentFactory::new(default.clone() as Arc<dyn LlmProvider>).with_provider(
            AgentName::Caspar,
            caspar_override.clone() as Arc<dyn LlmProvider>,
        );

        let agents = factory.create_agents(Mode::CodeReview);

        let melchior = agents
            .iter()
            .find(|a| a.name() == AgentName::Melchior)
            .unwrap();
        let balthasar = agents
            .iter()
            .find(|a| a.name() == AgentName::Balthasar)
            .unwrap();
        let caspar = agents
            .iter()
            .find(|a| a.name() == AgentName::Caspar)
            .unwrap();

        assert_eq!(melchior.provider_name(), "default");
        assert_eq!(balthasar.provider_name(), "default");
        assert_eq!(caspar.provider_name(), "caspar-special");
    }

    // -- BDD Scenario 30: modes generate different prompts --

    /// CodeReview, Design, Analysis produce distinct system prompts per agent.
    #[test]
    fn test_different_modes_produce_distinct_prompts() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;

        let cr = Agent::new(AgentName::Melchior, Mode::CodeReview, provider.clone());
        let design = Agent::new(AgentName::Melchior, Mode::Design, provider.clone());
        let analysis = Agent::new(AgentName::Melchior, Mode::Analysis, provider.clone());

        assert_ne!(cr.system_prompt(), design.system_prompt());
        assert_ne!(cr.system_prompt(), analysis.system_prompt());
        assert_ne!(design.system_prompt(), analysis.system_prompt());
    }

    // -- BDD Scenario 31: from_directory with nonexistent path --

    /// from_directory returns MagiError::Io for nonexistent directory.
    #[test]
    fn test_from_directory_returns_io_error_for_nonexistent_path() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let factory = AgentFactory::new(provider);
        let result = factory.from_directory(Path::new("/nonexistent/path"));
        assert!(matches!(result, Err(MagiError::Io(_))));
    }

    // -- Agent construction and accessors --

    /// Agent::new generates system prompt from include_str! prompts.
    #[test]
    fn test_agent_new_generates_system_prompt() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let agent = Agent::new(AgentName::Melchior, Mode::CodeReview, provider);
        assert!(!agent.system_prompt().is_empty());
    }

    /// Agent::with_custom_prompt uses provided prompt.
    #[test]
    fn test_agent_with_custom_prompt_uses_provided_prompt() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let agent = Agent::with_custom_prompt(
            AgentName::Melchior,
            Mode::CodeReview,
            provider,
            "Custom prompt".to_string(),
        );
        assert_eq!(agent.system_prompt(), "Custom prompt");
    }

    /// Agent::execute delegates to provider.complete with system prompt.
    #[tokio::test]
    async fn test_agent_execute_delegates_to_provider() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "response text"));
        let provider_arc = provider.clone() as Arc<dyn LlmProvider>;
        let agent = Agent::new(AgentName::Melchior, Mode::CodeReview, provider_arc);
        let config = CompletionConfig::default();

        let result = agent.execute("user input", &config).await;
        assert_eq!(result.unwrap(), "response text");
        assert_eq!(provider.calls(), 1);
    }

    /// Agent accessors return correct values.
    #[test]
    fn test_agent_accessors() {
        let provider = Arc::new(MockProvider::new("test-provider", "test-model", "r"));
        let provider_arc = provider.clone() as Arc<dyn LlmProvider>;
        let agent = Agent::new(AgentName::Balthasar, Mode::Design, provider_arc);

        assert_eq!(agent.name(), AgentName::Balthasar);
        assert_eq!(agent.mode(), Mode::Design);
        assert_eq!(agent.provider_name(), "test-provider");
        assert_eq!(agent.provider_model(), "test-model");
        assert_eq!(agent.display_name(), "Balthasar");
        assert_eq!(agent.title(), "Pragmatist");
    }

    // -- AgentFactory tests --

    /// AgentFactory::new creates 3 agents sharing default provider.
    #[test]
    fn test_agent_factory_creates_three_agents() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let factory = AgentFactory::new(provider);
        let agents = factory.create_agents(Mode::CodeReview);

        assert_eq!(agents.len(), 3);

        let names: Vec<AgentName> = agents.iter().map(|a| a.name()).collect();
        assert!(names.contains(&AgentName::Melchior));
        assert!(names.contains(&AgentName::Balthasar));
        assert!(names.contains(&AgentName::Caspar));
    }

    /// AgentFactory::create_agents returns agents in order [Melchior, Balthasar, Caspar].
    #[test]
    fn test_agent_factory_creates_agents_in_order() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let factory = AgentFactory::new(provider);
        let agents = factory.create_agents(Mode::CodeReview);

        assert_eq!(agents[0].name(), AgentName::Melchior);
        assert_eq!(agents[1].name(), AgentName::Balthasar);
        assert_eq!(agents[2].name(), AgentName::Caspar);
    }

    /// AgentFactory::with_provider overrides provider for specific agent.
    #[test]
    fn test_agent_factory_with_provider_overrides_specific_agent() {
        let default = Arc::new(MockProvider::new("default", "m1", "r1")) as Arc<dyn LlmProvider>;
        let override_p =
            Arc::new(MockProvider::new("override", "m2", "r2")) as Arc<dyn LlmProvider>;

        let factory = AgentFactory::new(default).with_provider(AgentName::Caspar, override_p);
        let agents = factory.create_agents(Mode::CodeReview);

        let caspar = agents
            .iter()
            .find(|a| a.name() == AgentName::Caspar)
            .unwrap();
        assert_eq!(caspar.provider_name(), "override");

        let melchior = agents
            .iter()
            .find(|a| a.name() == AgentName::Melchior)
            .unwrap();
        assert_eq!(melchior.provider_name(), "default");
    }

    /// AgentFactory::with_custom_prompt overrides prompt for specific agent.
    #[test]
    fn test_agent_factory_with_custom_prompt_overrides_prompt() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;

        let factory = AgentFactory::new(provider)
            .with_custom_prompt(AgentName::Melchior, "My custom prompt".to_string());
        let agents = factory.create_agents(Mode::CodeReview);

        let melchior = agents
            .iter()
            .find(|a| a.name() == AgentName::Melchior)
            .unwrap();
        assert_eq!(melchior.system_prompt(), "My custom prompt");

        let balthasar = agents
            .iter()
            .find(|a| a.name() == AgentName::Balthasar)
            .unwrap();
        assert_ne!(balthasar.system_prompt(), "My custom prompt");
        assert!(!balthasar.system_prompt().is_empty());
    }

    /// AgentFactory::create_agents returns exactly 3 agents for all modes.
    #[test]
    fn test_agent_factory_creates_three_agents_for_all_modes() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let factory = AgentFactory::new(provider);

        for mode in [Mode::CodeReview, Mode::Design, Mode::Analysis] {
            let agents = factory.create_agents(mode);
            assert_eq!(agents.len(), 3, "Expected 3 agents for mode {mode}");
        }
    }

    /// Default prompts contain JSON schema instructions and English constraint.
    #[test]
    fn test_default_prompts_contain_json_and_english_constraints() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;

        for name in [AgentName::Melchior, AgentName::Balthasar, AgentName::Caspar] {
            for mode in [Mode::CodeReview, Mode::Design, Mode::Analysis] {
                let agent = Agent::new(name, mode, provider.clone());
                let prompt = agent.system_prompt();
                assert!(
                    prompt.contains("JSON"),
                    "{name:?}/{mode:?} prompt should mention JSON"
                );
                assert!(
                    prompt.contains("English"),
                    "{name:?}/{mode:?} prompt should mention English"
                );
            }
        }
    }

    /// from_file with nonexistent path returns MagiError::Io.
    #[test]
    fn test_from_file_returns_io_error_for_nonexistent_path() {
        let provider = Arc::new(MockProvider::new("mock", "m1", "r1")) as Arc<dyn LlmProvider>;
        let result = Agent::from_file(
            AgentName::Melchior,
            Mode::CodeReview,
            provider,
            Path::new("/nonexistent/prompt.md"),
        );
        assert!(matches!(result, Err(MagiError::Io(_))));
    }
}
