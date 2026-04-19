# Especificacion: magi-core

## Objetivo

Implementar un crate Rust (`magi-core`) que proporcione un sistema de analisis
multi-perspectiva LLM-agnostico. El crate lanza tres sub-agentes independientes
(Melchior, Balthasar, Caspar) en paralelo contra cualquier proveedor LLM,
recolecta sus veredictos estructurados en JSON, computa un consenso por votacion
ponderada y genera un reporte unificado con hallazgos deduplicados, opinion
disidente y recomendaciones.

El sistema es un port conceptual del plugin MAGI de Python para Claude Code,
rediseñado para ser agnostico al proveedor LLM mediante un trait `LlmProvider`.

## Contexto del proyecto

- Proyecto: magi-core
- Lenguaje/Stack: Rust (edition 2024)
- Dependencias principales: tokio, reqwest, serde, serde_json, async-trait, thiserror, regex
- El crate es una libreria (`lib`), no un binario
- El crate recibe: modo de analisis + contenido (texto o path) + proveedor LLM configurado
- El crate devuelve: `MagiReport` con consenso, hallazgos, disidencias y reporte formateado

## Requerimientos funcionales (SDD)

### RF-01: Tipos de dominio (schema)

- Definir enums con comportamiento encapsulado via `impl`:
  - `Verdict` (Approve, Reject, Conditional):
    - `fn weight(&self) -> f64` — retorna peso de votacion (+1.0, +0.5, -1.0)
    - `fn effective(&self) -> Verdict` — mapea Conditional → Approve para conteo de mayoria
    - Implementar `Display` para formato uppercase en reportes ("APPROVE", "REJECT", "CONDITIONAL")
  - `Severity` (Critical, Warning, Info):
    - Implementar `Ord` para comparacion directa (Critical > Warning > Info)
    - `fn icon(&self) -> &str` — retorna icono de reporte ("[!!!]", "[!!]", "[i]")
    - Implementar `Display` para formato uppercase
  - `Mode` (CodeReview, Design, Analysis):
    - Implementar `Display` para formato de prompt ("code-review", "design", "analysis")
  - `AgentName` (Melchior, Balthasar, Caspar):
    - `fn title(&self) -> &str` — retorna rol (Scientist, Pragmatist, Critic)
    - `fn display_name(&self) -> &str` — retorna nombre capitalizado
    - Implementar `Display`, `Ord` (orden alfabetico para desempate determinista)
- Definir structs con estado y comportamiento:
  - `Finding` — campos: severity, title, detail
    - `fn stripped_title(&self) -> String` — retorna titulo sin caracteres zero-width Unicode
  - `AgentOutput` — campos: agent, verdict, confidence, summary, reasoning, findings, recommendation
    - `fn is_approving(&self) -> bool` — true si verdict es Approve o Conditional
    - `fn is_dissenting(&self, majority: &Verdict) -> bool` — true si effective verdict difiere de majority
    - `fn effective_verdict(&self) -> Verdict` — delega a `self.verdict.effective()`
- Todos los tipos deben implementar Serialize/Deserialize via serde, Clone, Debug, PartialEq

### RF-02: Validacion de schema (validate)

- Definir struct `ValidationLimits` — estado configurable con los umbrales de validacion:
  - Campos: `max_findings: usize`, `max_title_len: usize`, `max_detail_len: usize`, `max_text_len: usize`, `confidence_min: f64`, `confidence_max: f64`
  - `impl Default for ValidationLimits` — retorna los valores estandar del sistema MAGI (100, 500, 10_000, 50_000, 0.0, 1.0)
  - `#[non_exhaustive]` — permite agregar campos en futuras versiones sin breaking change
- Definir struct `Validator` con estado real:
  - Campo: `limits: ValidationLimits` — los umbrales activos para esta instancia
  - Campo: `zero_width_pattern: Regex` — patron compilado una sola vez en el constructor para strip de caracteres zero-width Unicode (categoria Cf)
  - `impl Validator`:
    - `fn new() -> Self` — constructor con `ValidationLimits::default()` y regex precompilado
    - `fn with_limits(limits: ValidationLimits) -> Self` — constructor con limites custom
    - `fn validate(&self, output: &AgentOutput) -> Result<(), MagiError>` — valida un AgentOutput completo. Invoca en orden:
      1. `validate_confidence(output.confidence)`
      2. `validate_text_field(output.summary, "summary")`
      3. `validate_text_field(output.reasoning, "reasoning")`
      4. `validate_text_field(output.recommendation, "recommendation")`
      5. `validate_findings(output.findings)`
    - `fn validate_confidence(&self, confidence: f64) -> Result<(), MagiError>` — verifica rango [confidence_min, confidence_max]
    - `fn validate_findings(&self, findings: &[Finding]) -> Result<(), MagiError>` — verifica conteo <= max_findings y cada finding via validate_finding()
    - `fn validate_finding(&self, finding: &Finding) -> Result<(), MagiError>` — verifica titulo (no vacio despues de strip_zero_width, longitud <= max_title_len) y detail (longitud <= max_detail_len)
    - `fn validate_text_field(&self, value: &str, field_name: &str) -> Result<(), MagiError>` — verifica longitud contra `max_text_len`; `field_name` se incluye en el mensaje de error para diagnóstico
    - `fn strip_zero_width(&self, text: &str) -> String` — usa el regex compilado en el estado
- Retornar `MagiError::Validation` con mensaje descriptivo en cada caso

### RF-03: Consenso ponderado (consensus)

- Definir struct `ConsensusConfig` — estado configurable del motor de consenso:
  - Campos: `min_agents: usize`, `epsilon: f64`
  - `impl Default for ConsensusConfig` — valores estandar: `min_agents = 2`, `epsilon = 1e-9`
  - `#[non_exhaustive]`
- Definir struct `ConsensusEngine` con estado configurable (sin estado mutable entre llamadas):
  - Campo: `config: ConsensusConfig` — configuracion activa
  - `impl ConsensusEngine`:
    - `fn new() -> Self` — constructor con `ConsensusConfig::default()`
    - `fn with_config(config: ConsensusConfig) -> Self` — constructor con config custom
    - `fn determine(&self, agents: &[AgentOutput]) -> Result<ConsensusResult, MagiError>` — punto de entrada principal; `&self` porque no muta estado. Requiere minimo `config.min_agents` agentes (retorna `MagiError::InsufficientAgents` si no). Rechaza nombres duplicados (retorna `MagiError::Validation` con mensaje "duplicate agent names"). Score y agent_count se retornan dentro de `ConsensusResult`
    - Metodos privados (SRP — un metodo por responsabilidad):
      - `fn compute_score(&self, verdicts: &[Verdict]) -> f64` — calcula `sum(v.weight() for v in verdicts) / verdicts.len()` (score normalizado en rango [-1.0, 1.0])
      - `fn classify(&self, score: f64, has_conditions: bool, effective_verdicts: &[Verdict], majority_verdict: &Verdict, num_agents: usize) -> (String, Verdict)` — mapea score a etiqueta + veredicto. `majority_verdict` y `effective_verdicts` se usan para calcular el conteo N-M en labels como "GO (2-1)" y "HOLD (2-1)", y para determinar el `consensus_verdict` del resultado
      - `fn compute_confidence(&self, majority: &[&AgentOutput], num_agents: usize, score: f64) -> f64` — formula de confianza
      - `fn deduplicate_findings(&self, agents: &[AgentOutput]) -> Vec<DedupFinding>` — merge por titulo case-insensitive, promueve severity, rastrea sources. Cuando se mergean dos findings: se conserva el detail del finding con mayor severity; si ambos tienen la misma severity, se conserva el del primer agente encontrado (orden: Melchior → Balthasar → Caspar, determinista por iteracion de Vec, no HashMap). **Limitacion conocida**: dos findings con el mismo titulo pero semantica distinta (ej. "Error handling" para panics vs logging) seran mergeados; el detail del finding perdedor se descarta. Se acepta como trade-off pragmatico — agregar discriminador secundario es trabajo futuro
      - `fn identify_sides(&self, agents: &[AgentOutput], majority_verdict: &Verdict) -> (Vec<&AgentOutput>, Vec<&AgentOutput>)` — separa mayoria y disidencia
- Definir struct `ConsensusResult` (Serialize) con campos:
  - `consensus: String` — label (ej. "GO (2-1)")
  - `consensus_verdict: Verdict` — veredicto final
  - `confidence: f64` — confianza calculada
  - `score: f64` — score ponderado calculado (para inspeccion por el caller)
  - `agent_count: usize` — numero de agentes que participaron en el consenso
  - `votes: HashMap<AgentName, Verdict>` — voto individual de cada agente
  - `majority_summary: String` — summaries de la mayoria unidos por " | "
  - `dissent: Vec<Dissent>` — agentes en minoria con summary y reasoning
  - `findings: Vec<DedupFinding>` — hallazgos deduplicados y ordenados por severity
  - `conditions: Vec<Condition>` — condiciones de aprobacion de agentes conditional
  - `recommendations: HashMap<AgentName, String>` — recomendacion por agente
- Definir struct `DedupFinding` (Serialize) — extiende Finding con campo `sources: Vec<AgentName>`
- Definir struct `Dissent` (Serialize) — campos: agent, summary, reasoning
- Definir struct `Condition` (Serialize) — campos: agent, condition
- Reglas de clasificacion (todas las comparaciones float usan `config.epsilon`):
  - `abs(score - 1.0) < epsilon` → label "STRONG GO", verdict Approve
  - `abs(score - (-1.0)) < epsilon` → label "STRONG NO-GO", verdict Reject
  - `score > epsilon` con condicionales → label "GO WITH CAVEATS" (sin conteo N-M, intencional), verdict Approve
  - `score > epsilon` sin condicionales → label "GO (N-M)" donde N=count(majority), M=count(minority), verdict Approve
  - `abs(score) < epsilon` → label "HOLD -- TIE", verdict Reject (empate favorece rechazo)
  - `score < -epsilon` → label "HOLD (N-M)", verdict Reject
- **Nota**: con `effective()` mapeando Conditional→Approve, los unicos effective verdicts posibles son {Approve, Reject}. Un 3-way split es imposible — siempre hay mayoria binaria. Este invariante simplifica la logica de `identify_sides()`
- **Cap en modo degradado**: cuando `agent_count < 3`, los labels STRONG GO y STRONG NO-GO se rebajan a GO (N-0) y HOLD (N-0) respectivamente, para evitar dar falsa confianza con un panel incompleto
- Formula de confianza:
  - `weight_factor = (abs(score) + 1) / 2`
  - `base_confidence = sum(confidencias_mayoria) / num_agentes` — nota: divide por num_agentes total (no solo mayoria). Esto es intencional: en un split 2-1, la confianza se diluye por el agente disidente aunque no contribuya al promedio. El efecto es que mayor disidencia reduce la confianza del consenso, lo cual refleja correctamente la incertidumbre. Replica el comportamiento del MAGI Python original.
  - `confidence = base_confidence * weight_factor`
  - Redondeado a 2 decimales, clamped a [0.0, 1.0]
- Mayoria: usa `Verdict::effective()` para mapear; empate de conteo se desempata por `AgentName::cmp()` (Ord)

### RF-04: Reporte (reporting)

- Definir struct `ReportConfig` — estado configurable del formateador:
  - Campos: `banner_width: usize`, `agent_titles: HashMap<AgentName, (String, String)>` (display_name, title)
  - `impl Default for ReportConfig` — width=52, titulos estandar MAGI (Melchior/Scientist, Balthasar/Pragmatist, Caspar/Critic)
  - `#[non_exhaustive]`
- Definir struct `ReportFormatter` con estado real:
  - Campo: `config: ReportConfig` — configuracion de formato activa
  - Campo: `banner_inner: usize` — calculado como `config.banner_width - 2` en constructor (evita recalcular en cada llamada)
  - `impl ReportFormatter`:
    - `fn new() -> Self` — constructor con `ReportConfig::default()`
    - `fn with_config(config: ReportConfig) -> Self` — constructor con config custom
    - `fn format_banner(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String` — genera banner ASCII de veredicto
    - `fn format_init_banner(&self, mode: &Mode, model: &str, timeout_secs: u64) -> String` — genera banner de inicializacion
    - `fn format_report(&self, agents: &[AgentOutput], consensus: &ConsensusResult) -> String` — genera reporte markdown completo (banner + todas las secciones)
    - Metodos privados (SRP — una seccion por metodo):
      - `fn format_separator(&self) -> String` — linea `"+" + "=" * inner + "+"`
      - `fn format_agent_line(&self, output: &AgentOutput) -> String` — linea de un agente en el banner
      - `fn format_consensus_summary(&self, consensus: &ConsensusResult) -> String`
      - `fn format_findings(&self, findings: &[DedupFinding]) -> String`
      - `fn format_dissent(&self, dissent: &[Dissent]) -> String`
      - `fn format_conditions(&self, conditions: &[Condition]) -> String`
      - `fn format_recommendations(&self, recommendations: &HashMap<AgentName, String>) -> String`
    - `fn agent_display(&self, name: &AgentName) -> (&str, &str)` — busca en `config.agent_titles`, fallback a `(name.display_name(), "Agent")`

El formato de salida debe replicar exactamente el del proyecto Python original.
Referencia: `D:\jbolivarg\PythonProjects\MAGI\skills\magi\scripts\reporting.py`

#### Banner ASCII

- Ancho fijo: 52 caracteres (inner = 50, bordes `|` a cada lado)
- Solo caracteres ASCII (evitar em-dash u otros multi-byte que desalinean en terminal)
- Estructura exacta:

```
+==================================================+
|          MAGI SYSTEM -- VERDICT                  |
+==================================================+
|  Melchior (Scientist):  APPROVE (90%)            |
|  Balthasar (Pragmatist):  CONDITIONAL (85%)      |
|  Caspar (Critic):  REJECT (78%)                  |
+==================================================+
|  CONSENSUS: GO WITH CAVEATS                      |
+==================================================+
```

- Formato de cada agente: `"  {Name} ({Title}):  {VERDICT} ({confidence:.0%})"`  justificado a la izquierda dentro del inner
- Titulo del header centrado dentro del inner
- Lineas separadoras: `"+" + "=" * 50 + "+"`

#### Banner de inicializacion (pre-analisis)

- Se emite antes de lanzar los agentes:

```
+==================================================+
|          MAGI SYSTEM -- INITIALIZING              |
+==================================================+
|  Mode: code-review
|  Model: sonnet (claude-sonnet-4-6)
|  Timeout: 120s
+==================================================+
```

#### Reporte markdown

Generar concatenando secciones en este orden exacto:

1. **Banner** (el ASCII box de arriba)
2. **Linea vacia**
3. **`## Consensus Summary`** — `majority_summary`: join de summaries de agentes en mayoria separados por ` | `, formato: `"Melchior: {summary} | Balthasar: {summary}"`
4. **Linea vacia**
5. **`## Key Findings`** (solo si hay findings) — cada finding:
   - `{icon} **[{SEVERITY}]** {title} _(from {sources})_`
   - `   {detail}`
   - Linea vacia
   - Iconos: `[!!!]` = critical, `[!!]` = warning, `[i]` = info, `[?]` = desconocido
   - Sources: join por coma de nombres de agentes que reportaron el finding
6. **`## Dissenting Opinion`** (solo si hay disidencia) — cada agente en minoria:
   - `**{Name} ({Title})**: {summary}`
   - `{reasoning}` (texto completo)
   - Linea vacia
7. **`## Conditions for Approval`** (solo si hay condiciones) — cada condicion:
   - `- **{Name}**: {condition}` (condition = recommendation del agente conditional)
   - Linea vacia al final
8. **`## Recommended Actions`** — cada agente:
   - `- **{Name}** ({Title}): {recommendation}`

#### Mapa de titulos de agentes

| agent key | Display Name | Title |
|-----------|-------------|-------|
| `melchior` | Melchior | Scientist |
| `balthasar` | Balthasar | Pragmatist |
| `caspar` | Caspar | Critic |
| desconocido | `{key.capitalize()}` | Agent |

### RF-05: Trait LlmProvider (provider)

- Definir trait asincrono `LlmProvider: Send + Sync` con metodos:
  - `async fn complete(&self, system_prompt: &str, user_prompt: &str, config: &CompletionConfig) -> Result<String, ProviderError>`
  - `fn name(&self) -> &str`
  - `fn model(&self) -> &str`
- Definir struct `CompletionConfig` con campos: `max_tokens: u32`, `temperature: f64`
  - `impl Default for CompletionConfig` — max_tokens=4096, temperature=0.0 (determinista para análisis estructurado)
  - `#[non_exhaustive]` — permite agregar campos en futuras versiones sin breaking change
  - **Nota**: `CompletionConfig` no tiene campo `timeout` — el timeout por agente se gestiona exclusivamente en `MagiConfig.timeout` y se aplica via `tokio::time::timeout` envolviendo cada `agent.execute()` en `launch_agents()`. Esto evita ambiguedad de precedencia entre dos timeouts
- Implementar struct `ClaudeProvider` (feature `claude`, v1.0):
  - Campos: `client: reqwest::Client`, `api_key: String`, `model: String`
  - `impl ClaudeProvider { fn new(api_key, model) -> Self }`
  - `impl LlmProvider for ClaudeProvider` — Claude Messages API (`/v1/messages`), header `x-api-key`
- Implementar struct `OpenAiProvider` (feature `openai`, v1.2 — no incluido en MVP):
  - Campos: `client: reqwest::Client`, `base_url: String`, `api_key: Option<String>`, `model: String`
  - `impl OpenAiProvider { fn new(base_url, api_key, model) -> Self }`
  - `impl LlmProvider for OpenAiProvider` — Chat Completions API (`/v1/chat/completions`), header `Authorization: Bearer {key}` (omitido si api_key es None)
  - `base_url` configurable permite reutilizar para LLMs locales (Ollama, llama.cpp, vLLM)
- Implementar struct `GeminiProvider` (feature `gemini`, v1.1 — no incluido en MVP):
  - Campos: `client: reqwest::Client`, `api_key: String`, `model: String`
  - `impl GeminiProvider { fn new(api_key, model) -> Self }`
  - `impl LlmProvider for GeminiProvider` — GenerateContent API, API key como query parameter `?key={key}`

**Scope v1.0 (MVP)**: solo `ClaudeProvider` (HTTP) + `ClaudeCliProvider` (CLI). Los demas providers se agregan en versiones posteriores: `GeminiProvider` + `GeminiCliProvider` en v1.1, `OpenAiProvider` en v1.2. El trait `LlmProvider` asegura que agregarlos no requiere cambios en el core.

### RF-06: Agentes (agent)

- Definir struct `Agent` con estado real — cada agente es una unidad autonoma con su propio provider:
  - Campo: `name: AgentName` — identidad del agente
  - Campo: `mode: Mode` — modo de operacion actual (determina perspectiva de analisis)
  - Campo: `system_prompt: String` — prompt activo, construido segun name + mode
  - Campo: `provider: Arc<dyn LlmProvider>` — proveedor LLM propio de este agente (Arc permite compartir el mismo provider entre multiples agentes via clone barato)
  - `impl Agent`:
    - `fn new(name: AgentName, mode: &Mode, provider: Arc<dyn LlmProvider>) -> Self` — constructor; genera el system prompt automaticamente segun name+mode
    - `fn with_custom_prompt(name: AgentName, mode: &Mode, provider: Arc<dyn LlmProvider>, prompt: String) -> Self` — constructor con prompt override
    - `fn from_file(name: AgentName, mode: &Mode, provider: Arc<dyn LlmProvider>, path: &Path) -> Result<Self, MagiError>` — carga prompt desde archivo
    - `async fn execute(&self, user_prompt: &str, config: &CompletionConfig) -> Result<String, ProviderError>` — ejecuta `self.provider.complete()` con el system prompt del agente; encapsula la responsabilidad de comunicacion con el LLM
    - `fn name(&self) -> &AgentName` — acceso al nombre
    - `fn mode(&self) -> &Mode` — acceso al modo activo
    - `fn system_prompt(&self) -> &str` — acceso al prompt construido
    - `fn provider_name(&self) -> &str` — delega a `self.provider.name()`
    - `fn provider_model(&self) -> &str` — delega a `self.provider.model()`
    - `fn display_name(&self) -> &str` — delega a `AgentName::display_name()`
    - `fn title(&self) -> &str` — delega a `AgentName::title()`
- Definir struct `AgentFactory` — responsable de crear conjuntos de agentes:
  - Campo: `default_provider: Arc<dyn LlmProvider>` — provider compartido; Arc permite clonar la referencia para cada agente sin copiar el objeto
  - Campo: `agent_providers: HashMap<AgentName, Arc<dyn LlmProvider>>` — providers especificos por agente (override del default)
  - Campo: `custom_prompts: HashMap<AgentName, String>` — overrides de prompts (vacio por defecto)
  - `impl AgentFactory`:
    - `fn new(default_provider: Arc<dyn LlmProvider>) -> Self` — factory con un provider compartido por los 3 agentes (Arc::clone para cada uno)
    - `fn with_provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self` — builder: asigna provider especifico a un agente
    - `fn with_custom_prompt(mut self, name: AgentName, prompt: String) -> Self` — builder: registra override de prompt
    - `fn from_directory(mut self, dir: &Path) -> Result<Self, MagiError>` — carga overrides de prompts desde directorio de archivos .md
    - `fn create_agents(&self, mode: &Mode) -> Vec<Agent>` — crea los 3 agentes para el modo dado, usando provider especifico o default, y custom prompts donde existan

  Ejemplo de uso con providers mixtos:
  ```rust
  let factory = AgentFactory::new(Arc::new(claude_provider))
      .with_provider(AgentName::Caspar, Arc::new(openai_provider))
      .with_custom_prompt(AgentName::Melchior, custom_prompt);
  ```

- Los system prompts default deben:
  - Instruir al agente a responder siempre en ingles
  - Especificar el esquema JSON exacto esperado como output
  - Definir la perspectiva especifica del agente segun el modo
- Los prompts default se almacenan como `const &str` embebidos en el codigo (un modulo por agente: `melchior.rs`, `balthasar.rs`, `caspar.rs`), cada uno con prompts por modo

### RF-07: Orquestador (orchestrator)

- Definir struct `MagiConfig` con estado configurable:
  - Campo: `timeout: Duration` — timeout por agente
  - Campo: `max_input_len: usize` — tamaño maximo del content en bytes (default: 1_048_576 = 1MB). Previene enviar payloads excesivos a multiples LLMs simultaneamente
  - Campo: `completion: CompletionConfig` — configuracion de completions pasada a cada agente (max_tokens, temperature)
  - `impl Default for MagiConfig` — timeout=300s, max_input_len=1_048_576, completion=CompletionConfig::default()
  - `#[non_exhaustive]` — permite agregar campos en futuras versiones sin breaking change

- Definir struct `MagiBuilder` — builder pattern para construccion ergonomica:
  - Estado interno acumulado: default_provider, agent_providers, custom_prompts, config, validator, consensus_engine, formatter (todos opcionales excepto default_provider)
  - `impl MagiBuilder`:
    - `fn new(provider: Arc<dyn LlmProvider>) -> Self` — inicia el builder con el provider default (requerido)
    - `fn provider(mut self, name: AgentName, provider: Arc<dyn LlmProvider>) -> Self` — asigna provider especifico a un agente
    - `fn custom_prompt(mut self, name: AgentName, prompt: String) -> Self` — override de system prompt
    - `fn prompts_dir(mut self, dir: &Path) -> Result<Self, MagiError>` — carga prompts custom desde directorio
    - `fn config(mut self, config: MagiConfig) -> Self` — config custom (incluye timeout, max_input_len, completion)
    - `fn completion(mut self, config: CompletionConfig) -> Self` — override de CompletionConfig sin reemplazar todo MagiConfig
    - `fn validation_limits(mut self, limits: ValidationLimits) -> Self` — limites de validacion custom
    - `fn consensus_config(mut self, config: ConsensusConfig) -> Self` — config de consenso custom
    - `fn report_config(mut self, config: ReportConfig) -> Self` — config de formato custom
    - `fn build(self) -> Magi` — construye el objeto Magi con todos los componentes

- Definir struct `Magi` como punto de entrada principal del crate — composicion de objetos con estado:
  - Campo: `config: MagiConfig` — configuracion del orquestador
  - Campo: `agent_factory: AgentFactory` — factory de agentes (con providers y prompts por agente)
  - Campo: `validator: Validator` — validador con limites y regex compilado
  - Campo: `consensus_engine: ConsensusEngine` — motor de consenso con config (stateless entre llamadas)
  - Campo: `formatter: ReportFormatter` — formateador con config de ancho y titulos
  - **Nota**: `Magi` no tiene un `provider` global — cada `Agent` tiene su propio provider. Esto permite mezclar providers (ej. Melchior en Claude, Caspar en GPT-4o, Balthasar en Gemini CLI)
  - `impl Magi`:
    - `fn new(provider: Arc<dyn LlmProvider>) -> Self` — constructor simple: un provider para los 3 agentes, todo default. Equivale a `Magi::builder(provider).build()`
    - `fn builder(provider: Arc<dyn LlmProvider>) -> MagiBuilder` — inicia el builder pattern
    - `async fn analyze(&self, mode: &Mode, content: &str) -> Result<MagiReport, MagiError>` — metodo principal; `&self` porque ConsensusEngine es stateless
    - Metodos privados:
      - `fn build_prompt(&self, mode: &Mode, content: &str) -> String` — construye `"MODE: {mode}\nCONTEXT:\n{content}"`
      - `async fn launch_agents(&self, agents: &[Agent], prompt: &str) -> Vec<Result<AgentOutput, MagiError>>` — lanza `agent.execute(prompt, config)` para cada agente en paralelo via `tokio::spawn`, con timeout por agente desde `config.timeout`. Cada agente usa su propio provider internamente.
      - `fn process_results(&self, results: Vec<Result<AgentOutput, MagiError>>) -> Result<(Vec<AgentOutput>, Vec<AgentName>), MagiError>` — separa exitos de fallos, verifica minimo de agentes

- Ejemplos de uso:

  ```rust
  // Caso simple — una linea, un provider, todo default
  let magi = Magi::new(Arc::new(ClaudeProvider::new(api_key, "claude-sonnet-4-6")));
  let report = magi.analyze(&Mode::CodeReview, content).await?;

  // Caso avanzado — providers mixtos, config custom
  let magi = Magi::builder(Arc::new(ClaudeProvider::new(key, "claude-opus-4-6")))
      .provider(AgentName::Caspar, Arc::new(OpenAiProvider::new(url, key, "gpt-4o")))
      .custom_prompt(AgentName::Melchior, my_scientist_prompt)
      .config(MagiConfig { timeout: Duration::from_secs(300) })
      .build();
  let report = magi.analyze(&Mode::Design, content).await?;

  // Caso desarrollo — CLI providers sin costo de API
  let magi = Magi::new(Arc::new(ClaudeCliProvider::new("sonnet")?));
  let report = magi.analyze(&Mode::Analysis, content).await?;
  ```

- El metodo `analyze` orquesta el flujo completo:
  1. Validar `content.len() <= config.max_input_len`; si excede, retornar `MagiError::InputTooLarge { size: usize, max: usize }`
  2. `agent_factory.create_agents(mode)` — obtiene los 3 agentes, cada uno con su provider y prompt segun modo
  3. `formatter.format_init_banner(mode, ...)` — genera banner de inicializacion
  4. `build_prompt(mode, content)` — construye prompt de usuario
  5. `launch_agents(agents, prompt)` — cada agente ejecuta `agent.execute()` en paralelo con su propio provider
  6. Deserializa cada respuesta JSON a `AgentOutput` via serde
  7. `validator.validate(output)` — valida cada AgentOutput
  8. `process_results(results)` — separa exitos/fallos, verifica minimo
  9. `consensus_engine.determine(successful)` — calcula consenso (stateless — score/count retornados en ConsensusResult)
  10. `formatter.format_report(successful, consensus)` — genera banner + reporte
  11. Construye y retorna `MagiReport`
- Degradacion elegante:
  - 2 de 3 agentes exitosos: continuar, marcar `degraded: true`, registrar `failed_agents`
  - Menos de 2 agentes exitosos: retornar `MagiError::InsufficientAgents`

### RF-08: Tipo de error unificado (error)

- Definir enum `MagiError` con variantes:
  - `Validation(String)` — schema invalido
  - `Provider(ProviderError)` — error del proveedor LLM
  - `InsufficientAgents { succeeded: usize, required: usize }` — menos de 2 agentes exitosos
  - `Deserialization(String)` — respuesta del LLM no es JSON valido
  - `InputTooLarge { size: usize, max: usize }` — el content excede max_input_len
  - `Io(std::io::Error)` — errores de I/O (lectura de archivos de prompt)
  - `impl MagiError`: implementar `std::fmt::Display` con mensajes descriptivos para cada variante
  - Derivar `thiserror::Error` para implementacion automatica de `std::error::Error`
  - Implementar `From<ProviderError>` para conversion automatica con `?`
  - Implementar `From<serde_json::Error>` para conversion a `Deserialization`
  - Implementar `From<std::io::Error>` para conversion a `Io`
- Definir enum `ProviderError` con variantes:
  - `Http { status: u16, body: String }` — respuesta HTTP no exitosa
  - `Network(String)` — error de red
  - `Timeout` — timeout agotado
  - `Auth(String)` — error de autenticacion
  - `Process { exit_code: Option<i32>, stderr: String }` — subproceso CLI fallo o retorno codigo no-cero
  - `NestedSession` — intento de lanzar CLI provider desde dentro de una sesion activa (ej. CLAUDECODE env var presente)
  - `impl ProviderError`: implementar `std::fmt::Display` con mensajes descriptivos
  - Derivar `thiserror::Error`

### RF-09: Reporte estructurado (MagiReport)

- Definir struct `MagiReport` con:
  - `agents: Vec<AgentOutput>` — respuestas individuales
  - `consensus: ConsensusResult` — resultado del consenso
  - `banner: String` — banner ASCII formateado
  - `report: String` — reporte markdown completo
  - `degraded: bool` — true si menos de 3 agentes respondieron
  - `failed_agents: Vec<AgentName>` — agentes que fallaron (solo si degraded = true)
- `MagiReport` debe implementar Serialize para exportar a JSON
- El JSON serializado debe replicar la estructura del proyecto Python original:

```json
{
  "agents": [
    {
      "agent": "melchior",
      "verdict": "approve",
      "confidence": 0.9,
      "summary": "One-line verdict",
      "reasoning": "Full analysis paragraphs...",
      "findings": [
        {
          "severity": "warning",
          "title": "Finding title",
          "detail": "Finding explanation..."
        }
      ],
      "recommendation": "What this agent recommends"
    }
  ],
  "consensus": {
    "consensus": "GO (2-1)",
    "consensus_verdict": "approve",
    "confidence": 0.85,
    "score": 0.33,
    "agent_count": 3,
    "votes": {
      "melchior": "approve",
      "balthasar": "conditional",
      "caspar": "reject"
    },
    "majority_summary": "Melchior: summary text | Balthasar: summary text",
    "dissent": [
      {
        "agent": "caspar",
        "summary": "Dissent one-liner",
        "reasoning": "Full dissent reasoning..."
      }
    ],
    "findings": [
      {
        "severity": "critical",
        "title": "Deduplicated finding",
        "detail": "Detail text...",
        "sources": ["melchior", "caspar"]
      }
    ],
    "conditions": [
      {
        "agent": "balthasar",
        "condition": "Recommendation from conditional agent"
      }
    ],
    "recommendations": {
      "melchior": "Recommendation text",
      "balthasar": "Recommendation text",
      "caspar": "Recommendation text"
    }
  },
  "degraded": false,
  "failed_agents": []
}
```

- El campo `degraded` solo aparece como `true` cuando al menos un agente fallo
- El campo `failed_agents` solo se incluye cuando `degraded` es `true`
- Los nombres de agentes en `votes`, `recommendations` y `sources` son lowercase (`melchior`, no `Melchior`)
- `consensus.confidence` se redondea a 2 decimales y se clampea a [0.0, 1.0]

### RF-10: Providers CLI para desarrollo local (cli-provider)

Referencia de implementacion: `D:\jbolivarg\RustProjects\PR-AI-Reviewer\src\backend\claude_code.rs`

- Implementar struct `ClaudeCliProvider` (feature `claude-cli`, v1.0):
  - Campos: `model: String`, `model_id: String`
  - `impl ClaudeCliProvider`:
    - `fn new(model: &str) -> Result<Self, ProviderError>` — constructor; acepta dos formatos de modelo: (1) alias corto contra whitelist (ej. "opus" → "claude-opus-4-6", "sonnet" → "claude-sonnet-4-6", "haiku" → "claude-haiku-4-5-20251001"), (2) model ID completo como pass-through (cualquier string que contenga "claude-" se acepta tal cual, permitiendo usar modelos nuevos sin actualizar el crate). Rechaza strings que no sean alias conocido ni model ID válido con `ProviderError::Auth`. Verifica que env var `CLAUDECODE` no este presente (retorna `ProviderError::NestedSession` si lo esta, fail-fast en constructor)
    - `fn build_args(&self, system_prompt: &str) -> Vec<String>` — construye argumentos CLI: `["--print", "--output-format", "json", "--model", model_id, "--system-prompt", system_prompt]`
    - `fn parse_cli_output(&self, raw: &[u8]) -> Result<String, ProviderError>` — parsea double-nested JSON de Claude Code
    - `fn extract_json(text: &str) -> &str` — strip code fences con `strip_prefix("```json")` / `strip_suffix("```")`
  - `impl LlmProvider for ClaudeCliProvider` — lanza `tokio::process::Command::new("claude")`, stdin/stdout/stderr piped, user prompt via stdin
  - Parseo double-nested: `{"type":"result","subtype":"success","is_error":false,"result":"...","usage":{...}}` → verificar `is_error` → extraer string `result` → strip fences → retornar
  - Definir struct auxiliar `CliOutput` (Deserialize) para el outer JSON

- Implementar struct `GeminiCliProvider` (feature `gemini-cli`, v1.1 — no incluido en MVP):
  - Campos: `model: String`
  - `impl GeminiCliProvider`:
    - `fn new(model: &str) -> Self` — constructor; valida alias contra whitelist
    - `fn build_args(&self, system_prompt: &str) -> Vec<String>` — argumentos para Gemini CLI
    - `fn parse_cli_output(&self, raw: &[u8]) -> Result<String, ProviderError>` — parsea respuesta Gemini CLI
  - `impl LlmProvider for GeminiCliProvider` — lanza `tokio::process::Command::new("gemini")`, misma mecanica de stdin/stdout

- Ambos providers implementan el trait `LlmProvider` — el orquestador no distingue si el backend es API HTTP o CLI
- Subprocesos lanzados con stdin/stdout/stderr como `Stdio::piped()`
- Timeout via `tokio::time::timeout` envolviendo `child.wait_with_output()`; si expira, se mata el proceso hijo con `child.kill()` y se retorna `ProviderError::Timeout`
- Verificar exit status del subproceso; si no es success, capturar stderr y retornar `ProviderError::Process`
- Estimacion de tokens: `response.len() / 4` (caracteres por token aproximado, misma heuristica que PR-AI-Reviewer)
- Caso de uso principal: desarrollo y testing usando suscripciones CLI existentes sin costo de API
- **Limitacion conocida (Windows)**: `child.kill()` invoca `TerminateProcess` que no propaga a grandchild processes. Si el CLI tool spawna subprocesos internos (ej. Claude Code spawna Node.js), esos pueden quedar huerfanos en timeout o si el proceso padre muere. Documentar como limitacion conocida en la API publica

## Restricciones

### Arquitectura

- El crate es una libreria; no incluye binario ni CLI
- No debe depender de ningun proveedor LLM especifico en tiempo de compilacion (los providers son features opcionales)
- Los modulos de logica pura (schema, validate, consensus, reporting) no deben tener dependencias async ni de red
- El orquestador usa tokio para async pero no expone el runtime al usuario (el usuario trae su propio runtime)

### Paradigma (ref: ~/.claude/CLAUDE.md — Programming Paradigm)

- Usar `struct` + `impl` + `trait` para polimorfismo; seguir ownership semantics
- Preferir OOP tradicional; funcional solo cuando no existe solucion OOP practica

### Calidad (ref: ~/.claude/CLAUDE.md — Quality)

- Diseño modular con bajo acoplamiento entre modulos
- Codigo reutilizable con alta cohesion; evitar implementaciones context-specific
- Interfaces claras y no ambiguas para todos los modulos; documentar contratos (inputs, outputs, side effects)
- Optimizar para menor complejidad temporal/espacial (Big O)
- DRY: no duplicar codigo
- SRP: un proposito por struct, una tarea por metodo
- Usar constantes con nombre; prohibidos magic numbers/strings (ej. los pesos de veredicto, anchos de banner, limites de longitud deben ser constantes nombradas)

### Documentacion (ref: ~/.claude/CLAUDE.md — Documentation)

- Todas las APIs publicas deben tener documentacion Rustdoc (`///`, `//!`)
- Docstrings requeridos para todos los structs, metodos y funciones: proposito, parametros, valores de retorno
- Comentarios inline solo para logica no obvia
- Incluir ejemplos de uso para structs y metodos no triviales
- Documentar posibles errores en cada funcion que retorne `Result`

### Estilo (ref: ~/.claude/CLAUDE.md — Style)

- Enforced por `rustfmt` y `clippy`
- Naming: `snake_case` funciones/variables, `PascalCase` tipos, `SCREAMING_SNAKE_CASE` constantes
- Longitud de linea razonable (80-120 caracteres)
- Organizar imports: std primero, luego externos, luego locales

### Error handling (ref: ~/.claude/CLAUDE.md — Error Handling)

- Manejar todos los casos de error explicitamente; no silent failures
- Usar `Result<T, E>` y `Option<T>`; propagar con `?`
- `panic!` esta **prohibido** en todo el crate — esto es una libreria publica; todos los errores se propagan via `Result<T, MagiError>`. Incluye `unwrap()`, `expect()`, `unreachable!()`, `todo!()` y cualquier otra macro que pueda causar panic en runtime. Unica excepcion: tests (`#[cfg(test)]`)
- `unsafe` esta prohibido

### Dependencias (ref: ~/.claude/CLAUDE.md — Dependencies)

- Preferir tipos de la libreria estandar sobre dependencias externas donde sea posible
- Justificar cada dependencia third-party
- Pinear versiones en Cargo.toml para reproducibilidad
- Ejecutar `cargo audit` para verificar vulnerabilidades conocidas
- Verificar compatibilidad de licencias

### Memoria (ref: ~/.claude/CLAUDE.md — Memory & Resources)

- Preferir stack sobre heap; dynamic allocation solo cuando el tamaño es desconocido en compile time
- Seguir ownership y borrowing semantics
- Usar Drop trait para cleanup automatico de recursos (ej. kill de subprocesos CLI en timeout)

### Seguridad (ref: ~/.claude/CLAUDE.md — Security)

- Validar todos los inputs; sanitizar antes de usar
- No hardcodear secrets ni credenciales (API keys se pasan por configuracion)
- Fail securely; usar defaults seguros
- Minimizar superficie de ataque: los CLI providers deben validar que el binario existe antes de lanzar subprocesos

### Testing (ref: ~/.claude/CLAUDE.md — Testing)

- Tests unitarios para todos los structs y metodos nuevos
- Cubrir edge cases: boundary values, inputs vacios, condiciones de error
- Nombres de tests descriptivos que documenten comportamiento esperado
- Tests aislados; no compartir estado entre tests
- Framework: built-in `#[test]` + `cargo nextest`
- TDD estricto: Red-Green-Refactor (enforced por TDD-Guard)

### File header (ref: ~/.claude/CLAUDE.md — proyecto)

- Todo archivo fuente nuevo debe iniciar con:
  ```rust
  // Author: Julian Bolivar
  // Version: 1.0.0
  // Date: YYYY-MM-DD
  ```

## Comportamiento esperado (BDD)

### Escenario 1: Analisis exitoso con 3 agentes unanimes en approve

- **Dado** un `LlmProvider` mock que retorna JSON valido con verdict "approve" y confidence 0.9 para los 3 agentes
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, "fn main() {}")`
- **Entonces** el `MagiReport` contiene:
  - 3 `AgentOutput` con verdicts approve
  - consensus_verdict = approve
  - consensus_label = "STRONG GO"
  - confidence cercana a 0.9
  - degraded = false

### Escenario 2: Analisis con consenso mixto (2 approve, 1 reject)

- **Dado** un provider mock donde Melchior y Balthasar retornan approve (0.85) y Caspar retorna reject (0.78)
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces** el consenso es:
  - score = (1 + 1 - 1) / 3 = 0.333...
  - consensus_label = "GO (2-1)"
  - consensus_verdict = approve
  - dissent contiene a Caspar con su reasoning
  - confidence < 0.85 (reducida por disidencia)

### Escenario 3: Empate con veredicto conditional

- **Dado** un provider mock donde Melchior retorna approve, Balthasar retorna conditional y Caspar retorna reject
- **Cuando** se ejecuta `magi.analyze(Mode::Design, content)`
- **Entonces** el consenso es:
  - score = (1 + 0.5 - 1) / 3 = 0.1666...
  - consensus_label = "GO WITH CAVEATS"
  - conditions contiene la recommendation de Balthasar
  - consensus_verdict = approve

### Escenario 4: Rechazo unanime

- **Dado** un provider mock donde los 3 agentes retornan reject con confidence 0.95
- **Cuando** se ejecuta `magi.analyze(Mode::Analysis, content)`
- **Entonces** el consenso es:
  - score = -1.0
  - consensus_label = "STRONG NO-GO"
  - consensus_verdict = reject
  - confidence cercana a 0.95
  - dissent esta vacio (no hay minoria)

### Escenario 5: Empate perfecto con 2 agentes sinteticos (score = 0)

- **Dado** input sintetico directo al modulo de consenso con 2 agentes: 1 approve + 1 reject
- **Nota**: con 3 agentes, score = 0 no es alcanzable con las combinaciones de pesos {+1, +0.5, -1}. Este escenario se testea con 2 agentes para verificar el comportamiento de empate.
- **Cuando** se calcula consenso con 2 agentes: 1 approve + 1 reject
- **Entonces** el consenso es:
  - score = 0
  - consensus_label = "HOLD -- TIE"
  - consensus_verdict = reject (empate favorece rechazo)

### Escenario 6: Degradacion elegante — 1 agente falla por timeout

- **Dado** un provider mock donde Melchior y Balthasar retornan approve pero Caspar excede el timeout
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces**:
  - El resultado contiene 2 AgentOutput (Melchior y Balthasar)
  - `degraded = true`
  - `failed_agents` contiene `[Caspar]`
  - El consenso se calcula con 2 agentes
  - No se retorna error — el `Result` es `Ok(MagiReport)`

### Escenario 7: Degradacion elegante — 1 agente falla por JSON invalido

- **Dado** un provider mock donde Melchior y Caspar retornan JSON valido pero Balthasar retorna texto plano no parseable
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces**:
  - El resultado contiene 2 AgentOutput (Melchior y Caspar)
  - `degraded = true`
  - `failed_agents` contiene `[Balthasar]`
  - El consenso se calcula con 2 agentes
  - No se retorna error

### Escenario 8: Error — solo 1 agente exitoso (2 fallan)

- **Dado** un provider mock donde solo Melchior retorna exitosamente y los otros 2 fallan (Balthasar por timeout, Caspar por error HTTP)
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces** retorna `Err(MagiError::InsufficientAgents { succeeded: 1, required: 2 })`

### Escenario 9: Error — los 3 agentes fallan

- **Dado** un provider mock donde los 3 agentes fallan (Melchior por timeout, Balthasar por error de red, Caspar por JSON invalido)
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces** retorna `Err(MagiError::InsufficientAgents { succeeded: 0, required: 2 })`

### Escenario 10: Validacion rechaza confidence fuera de rango

- **Dado** un `AgentOutput` con confidence = 1.5
- **Cuando** se valida con `validator.validate(&output)`
- **Entonces** retorna `Err(MagiError::Validation)` con mensaje indicando "confidence out of range"

### Escenario 11: Validacion rechaza titulo vacio despues de strip zero-width

- **Dado** un `AgentOutput` con un finding cuyo titulo contiene solo caracteres zero-width Unicode (U+200B, U+FEFF)
- **Cuando** se valida con `validator.validate(&output)`
- **Entonces** retorna `Err(MagiError::Validation)` con mensaje indicando "empty title"

### Escenario 12: Validacion rechaza text field que excede max_text_len

- **Dado** un `AgentOutput` con `reasoning` de 50,001 caracteres (excede `MAX_TEXT_LEN = 50_000`)
- **Cuando** se valida con `validator.validate(&output)`
- **Entonces** retorna `Err(MagiError::Validation)` con mensaje indicando "reasoning exceeds maximum length"

### Escenario 13: Deduplicacion de findings por titulo

- **Dado** 3 AgentOutputs donde Melchior y Caspar reportan un finding con el mismo titulo (diferente case) pero diferente severity (warning vs critical)
- **Cuando** se calcula consenso
- **Entonces** los findings deduplicados contienen:
  - Un solo finding con ese titulo
  - Severity promovida a critical (la mas alta)
  - Sources incluye ambos agentes (Melchior y Caspar)

### Escenario 14: Respuesta del LLM no es JSON valido

- **Dado** un provider mock que retorna texto plano en vez de JSON
- **Cuando** el orquestador intenta parsear la respuesta
- **Entonces** ese agente se trata como fallido (MagiError::Deserialization) y el sistema continua con los agentes restantes si hay suficientes

### Escenario 15: Banner ASCII tiene ancho fijo de 52 caracteres

- **Dado** un MagiReport generado con cualquier combinacion de veredictos
- **Cuando** se inspecciona el banner
- **Entonces** todas las lineas del banner tienen exactamente 52 caracteres de ancho

### Escenario 16: Reporte markdown contiene todas las secciones

- **Dado** un MagiReport con consenso mixto (approve + conditional + reject)
- **Cuando** se inspecciona el campo report
- **Entonces** contiene los headers markdown:
  - `## Consensus Summary`
  - `## Key Findings`
  - `## Dissenting Opinion`
  - `## Conditions for Approval`
  - `## Recommended Actions`

### Escenario 17: Provider con base_url custom para LLM local (v1.2)

- **Dado** un `OpenAiProvider` configurado con base_url = "http://localhost:11434/v1" y api_key = None
- **Cuando** se construye el provider
- **Entonces** el provider se crea exitosamente sin error de autenticacion
- **Y** `provider.name()` retorna "openai"
- **Y** el request no incluye header Authorization

### Escenario 18: ClaudeCliProvider lanza 3 subprocesos en paralelo

- **Dado** un `ClaudeCliProvider` configurado con modelo "sonnet"
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)` (usando mock de subprocess)
- **Entonces**:
  - Se lanzan exactamente 3 subprocesos `claude` en paralelo
  - Cada subproceso recibe un system prompt distinto (Melchior, Balthasar, Caspar)
  - El user prompt se envia via stdin, no como argumento CLI
  - Cada subproceso incluye los flags `--output-format json` y `--model`

### Escenario 19: ClaudeCliProvider parsea double-nested JSON

- **Dado** un subproceso claude que retorna `{"type":"result","subtype":"success","is_error":false,"result":"{\"agent\":\"melchior\",\"verdict\":\"approve\",...}","usage":{"input_tokens":100}}`
- **Cuando** el provider parsea la respuesta
- **Entonces** deserializa el outer JSON, verifica `is_error == false`, extrae el string `result`, lo deserializa como JSON y retorna el contenido

### Escenario 20: ClaudeCliProvider detecta error en respuesta CLI

- **Dado** un subproceso claude que retorna `{"type":"result","subtype":"error","is_error":true,"result":"Rate limit exceeded"}`
- **Cuando** el provider parsea la respuesta
- **Entonces** retorna `Err(ProviderError::Process)` con el mensaje de error del campo `result`

### Escenario 21: ClaudeCliProvider strip code fences del JSON interno

- **Dado** un subproceso claude cuyo campo `result` contiene el JSON envuelto en ` ```json\n{...}\n``` `
- **Cuando** el provider parsea la respuesta
- **Entonces** aplica `strip_prefix("```json")` / `strip_suffix("```")` y retorna solo el JSON limpio

### Escenario 22: ClaudeCliProvider maneja timeout de subproceso

- **Dado** un `ClaudeCliProvider` con timeout de 5 segundos y un subproceso que no termina
- **Cuando** el timeout expira via `tokio::time::timeout`
- **Entonces**:
  - El proceso hijo es terminado con `child.kill()`
  - El provider retorna `Err(ProviderError::Timeout)`
  - El agente se marca como fallido y el sistema continua con los restantes

### Escenario 23: ClaudeCliProvider detecta sesion anidada en constructor

- **Dado** que la variable de entorno `CLAUDECODE` esta definida
- **Cuando** se intenta construir `ClaudeCliProvider::new("sonnet")`
- **Entonces** retorna `Err(ProviderError::NestedSession)` sin crear el provider (fail-fast en constructor)

### Escenario 24: GeminiCliProvider funciona como provider valido (v1.1)

- **Dado** un `GeminiCliProvider` configurado con un modelo valido
- **Cuando** se ejecuta `provider.complete(system_prompt, user_prompt, config)` (usando mock de subprocess)
- **Entonces**:
  - Lanza un subproceso `gemini` con los flags correctos
  - Envia el user prompt via stdin
  - Parsea la respuesta y retorna el JSON del agente

### Escenario 25: OpenAiProvider con base_url local no requiere API key (v1.2)

- **Dado** un `OpenAiProvider` con base_url = "http://localhost:11434/v1" y api_key = None
- **Cuando** se ejecuta `provider.complete(system_prompt, user_prompt, config)`
- **Entonces** el request HTTP no incluye header `Authorization`

### Escenario 26: Agentes con providers distintos

- **Dado** un `AgentFactory` configurado con:
  - Melchior usando un mock provider que retorna approve con confidence 0.9
  - Balthasar usando un mock provider diferente que retorna conditional con confidence 0.8
  - Caspar usando un tercer mock provider que retorna reject con confidence 0.7
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces**:
  - Cada agente invoca su propio provider (verificar que cada mock recibe exactamente 1 llamada)
  - El consenso se calcula normalmente con los 3 resultados
  - El reporte incluye los 3 agentes independientemente del provider usado

### Escenario 27: AgentFactory con provider default y override por agente

- **Dado** un `AgentFactory::new(default_provider).with_provider(AgentName::Caspar, caspar_provider)`
- **Cuando** se crean los agentes con `factory.create_agents(mode)`
- **Entonces**:
  - Melchior y Balthasar reciben el default_provider
  - Caspar recibe caspar_provider
  - Los 3 agentes tienen system prompts correctos para el modo

### Escenario 28: Constructor simple Magi::new con un solo provider

- **Dado** un mock provider
- **Cuando** se ejecuta `Magi::new(Arc::new(mock_provider))`
- **Entonces**:
  - El objeto Magi se crea exitosamente
  - Los 3 agentes internos comparten el mismo provider
  - Config, validator, consensus_engine y formatter usan defaults

### Escenario 29: Builder con providers mixtos y config custom

- **Dado** un builder con provider default (mock_claude) y override para Caspar (mock_openai), timeout de 300s
- **Cuando** se ejecuta `Magi::builder(mock_claude).provider(Caspar, mock_openai).config(config).build()`
- **Entonces**:
  - Melchior y Balthasar usan mock_claude
  - Caspar usa mock_openai
  - El timeout es 300s
  - Los demas componentes usan defaults

### Escenario 30: Multiples modos generan prompts distintos

- **Dado** los modos CodeReview, Design y Analysis
- **Cuando** se obtienen los agentes default para cada modo via `AgentFactory::create_agents(mode)`
- **Entonces** cada modo produce system prompts con contenido diferente que refleja la perspectiva de analisis del modo

### Escenario 31: AgentFactory::from_directory con directorio inexistente

- **Dado** un `AgentFactory` donde se llama `from_directory(Path::new("/nonexistent"))`
- **Cuando** se intenta cargar los prompts custom
- **Entonces** retorna `Err(MagiError::Io)` con el error del filesystem

### Escenario 32: Error por input demasiado grande (InputTooLarge)

- **Dado** un `Magi` con `MagiConfig { max_input_len: 1000, .. }`
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, &"x".repeat(1001))`
- **Entonces** retorna `Err(MagiError::InputTooLarge { size: 1001, max: 1000 })` sin lanzar ningun agente

### Escenario 33: Degraded mode cap de labels STRONG

- **Dado** un provider mock donde Melchior y Balthasar retornan approve (0.9) pero Caspar falla por timeout
- **Cuando** se ejecuta `magi.analyze(Mode::CodeReview, content)`
- **Entonces**:
  - degraded = true
  - consensus_label = "GO (2-0)" (no "STRONG GO" — cap por modo degradado)
  - agent_count = 2 en ConsensusResult

## Lo que NO debe hacer

- No debe incluir un binario CLI — es solo una libreria
- No debe forzar un runtime async especifico (el usuario trae su propio `#[tokio::main]`)
- No debe hardcodear API keys ni URLs — todo se pasa por configuracion
- No debe hacer retry automatico a nivel de orquestador (el usuario controla retries en su provider)
- No debe modificar el input del usuario (no sanitizar, no truncar)
- No debe almacenar estado mutable entre llamadas a `analyze()` — cada llamada es independiente; `ConsensusEngine` es stateless (score/count se retornan en `ConsensusResult`)
- No debe depender de acceso a filesystem excepto para cargar system prompts custom (opcional)
- No debe incluir los providers en la compilacion por defecto — deben ser features opcionales:
  - HTTP: `features = ["claude", "openai", "gemini"]`
  - CLI: `features = ["claude-cli", "gemini-cli"]`
- No debe hacer logging directo — usar trait o callback si se necesita observabilidad
- Los CLI providers no deben lanzarse desde dentro de una sesion de Claude Code (detectar variable `CLAUDECODE` y retornar error claro)
