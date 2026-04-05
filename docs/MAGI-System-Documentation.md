# MAGI System — Complete Technical Documentation

## Multi-Perspective Analysis Library for Rust

---

## 1. Origin: The MAGI Supercomputers from Evangelion

### 1.1 Context in the Series

In *Neon Genesis Evangelion* (1995), created by Hideaki Anno and produced by Gainax, **NERV** — the paramilitary organization tasked with defending humanity against the Angels — operates with a system of three supercomputers known as the **MAGI**.

The MAGI were designed and built by **Dr. Naoko Akagi**, NERV's chief scientist and mother of Ritsuko Akagi. The system takes its name from the three Magi of the biblical account: Melchior, Balthasar, and Caspar (the wise men who traveled to Bethlehem guided by a star). The naming is deliberate: just as the three wise men brought distinct perspectives and offerings, the three computers contribute complementary facets to the decision-making process.

### 1.2 The Three Units

Each supercomputer contains a copy of Naoko Akagi's personality, but filtered through a different aspect of her identity:

| Unit           | Aspect of Naoko      | Nature                                           |
|--------------- |--------------------- |------------------------------------------------- |
| **MELCHIOR-1** | As a scientist       | Analytical, rigorous, truth-oriented              |
| **BALTHASAR-2**| As a mother          | Protective, pragmatic, welfare-oriented           |
| **CASPAR-3**   | As a woman           | Intuitive, survival-oriented, risk-aware          |

### 1.3 Decision Mechanism

The MAGI operate by **majority vote**: each unit issues an independent verdict on NERV's critical decisions, and the outcome is determined by consensus of at least two out of three. This mechanism appears at crucial moments in the series, such as when NERV must decide whether to self-destruct the base during the Angel Iruel's invasion (episode 13), or during SEELE's hacking attempt on the MAGI in *The End of Evangelion*.

The narrative brilliance of the system is that the three units can reach **different conclusions** from the same input, because each processes information through a distinct cognitive filter. The conflict between the three is not a bug: it is the mechanism that produces more robust decisions than any single perspective.

### 1.4 The Philosophical Principle

Behind the MAGI lies a profound idea: **no single perspective is sufficient for good decision-making under uncertainty**. The scientist may have the technically correct but impractical answer. The mother may prioritize safety at the cost of truth. The woman may perceive risks that the other two ignore. It is in the deliberate tension between the three that wisdom emerges.

This principle has roots in real decision theory concepts: Surowiecki's *Wisdom of Crowds*, *ensemble methods* in machine learning, the structure of multi-judge panels in legal systems, and the military practice of red-teaming where a dedicated team argues the adversary's position to stress-test strategy.

### 1.5 Why Structured Disagreement Works

The effectiveness of multi-perspective systems rests on three conditions identified by decision theory research:

1. **Diversity of perspective** — Each evaluator must genuinely see the problem differently, not just apply the same analysis with different labels. MAGI achieves this through radically different system prompts that define what each agent prioritizes and ignores.

2. **Independence of judgment** — Evaluators must form opinions without knowing what the others concluded. Anchoring (adjusting your opinion toward what others already said) is the primary destroyer of multi-perspective value. MAGI enforces this by running agents in parallel with no shared context.

3. **Structured aggregation** — Raw disagreement is noise. Value comes from a synthesis mechanism that weights votes, preserves dissent, and surfaces the *reasons* behind disagreement. MAGI's weight-based scoring and findings deduplication serve this role.

When these conditions hold, the system consistently outperforms any individual evaluator — not because it is smarter, but because it is more complete.

---

## 2. Translation to the Software Engineering Domain

### 2.1 Conceptual Mapping

magi-core takes Evangelion's architecture and adapts it to the software development context, replacing Naoko's personality aspects with complementary **professional lenses**:

| Evangelion               | magi-core                 | Lens                                    |
|------------------------- |-------------------------- |---------------------------------------- |
| Naoko as scientist       | **Melchior** (Scientist)  | Technical rigor, correctness, efficiency |
| Naoko as mother          | **Balthasar** (Pragmatist)| Practicality, maintainability, team      |
| Naoko as woman           | **Caspar** (Critic)       | Risk, edge cases, failure modes          |

The adaptation preserves the fundamental property of the original system: each agent analyzes exactly the same input, but through a radically different cognitive filter, and the disagreement between them is valuable information, not noise.

### 2.2 Why Three Perspectives and Not Two or Five

Three is the minimum number that allows majority voting without deadlock. With two agents, a disagreement produces a tie with no resolution mechanism. With five, computational cost triples without a proportional improvement in decision quality (diminishing returns). Three also allows each agent to have a strong, differentiated identity, while five would dilute the perspectives into overlapping concerns.

### 2.3 Addressing Cognitive Biases

The adversarial multi-perspective model addresses well-documented cognitive biases in software engineering:

| Bias | How MAGI Mitigates It |
|------|----------------------|
| **Confirmation bias** | Three agents with different evaluation criteria are unlikely to share the same blind spots |
| **Anchoring** | Agents analyze independently — no agent sees the others' output before forming its own verdict |
| **Groupthink** | Caspar (Critic) is designed to be adversarial; its role is to find fault, not agree |
| **Optimism bias** | The weight-based scoring penalizes reject (-1) more heavily than approve (+1), making negative signals harder to override |
| **Status quo bias** | Each agent evaluates from first principles against its own criteria, not against "how things are done" |
| **Overconfidence** | The confidence formula produces lower scores when agents disagree, surfacing genuine uncertainty |

---

## 3. The Three Agents in Detail

### 3.1 Melchior — The Scientist

**Philosophy:** "Is this correct? Is this optimal?"

Melchior embodies the rigor of a principal engineer or research scientist who prioritizes technical truth above all else. It doesn't care if the solution is easy to implement or if the team understands it — it cares if it is *correct*.

**In code review** it analyzes: logical errors, algorithmic complexity (O(n) vs O(n^2)), type safety, correct use of ownership/lifetimes in Rust, test coverage.

**In design** it evaluates: theoretical soundness of the architecture, formal properties (consistency, deadlock-freedom), API and interface quality, analytical scalability.

**In general analysis** it seeks: the real root cause beneath symptoms, hard constraints (memory, timing, bandwidth), first-principles reasoning, concrete evidence.

**Personality:** Precise, cites specific evidence (line numbers, data, specs). If uncertain, it says so explicitly and explains what information would resolve the uncertainty. Prefers proven solutions over clever ones.

### 3.2 Balthasar — The Pragmatist

**Philosophy:** "Does this work in practice? Can we live with this?"

Balthasar is the experienced tech lead who has seen enough projects die from over-engineering to deeply value simplicity. It thinks in trade-offs, not absolutes.

**In code review** it analyzes: readability for a new team member in 6 months, unnecessary coupling, appropriate level of abstraction (neither too much nor too little), documentation of the "why", impact on team conventions.

**In design** it evaluates: realistic implementation time, migration cost from the current state, team capability to build and maintain this, operational burden (deploy, monitoring, debugging), reversibility if it turns out to be the wrong choice.

**In general analysis** it seeks: real user/business impact, cost/benefit ratio, precedents (has someone solved this before?), the incremental path (80% of the value with 20% of the effort), external dependencies that could block progress.

**Personality:** Grounded, trade-off oriented. Asks "what's the simplest thing that could work?" before reaching for complexity. Detects over-engineering and yak-shaving with ease.

### 3.3 Caspar — The Critic

**Philosophy:** "How does this break? What aren't we seeing?"

Caspar is the system's deliberate adversary. It functions as an internal red team: its job is to try to break everything the other two approved. It is not negative for sport — it is negative by design, because someone has to be.

**In code review** it analyzes: unconsidered edge cases (null, empty, overflow, unicode, concurrency, power loss mid-operation), security vulnerabilities (injection, buffer overflow, TOCTOU, privilege escalation), failure modes (what happens when this fails? is it graceful?), implicit assumptions, regression risk.

**In design** it evaluates: attack surface, failure scenarios (what happens if component X goes down? if the network partitions?), the "scaling cliff" (at what load does this design break?), hidden coupling, the worst possible case.

**In general analysis** it seeks: blind spots, adversarial thinking ("if someone wanted this to fail, how would they do it?"), historical parallels of similar failures, second-order effects, audit of fragile assumptions.

**Personality:** Direct, doesn't sugarcoat. Distinguishes between theoretical risks and likely risks (labels both honestly). It is the agent most likely to vote "reject" — and that is a feature, not a bug. When it genuinely cannot find serious issues, it says so with confidence.

---

## 4. Library Architecture

### 4.1 Module Structure

```
lib.rs (crate root)
├── error.rs          — ProviderError + MagiError enums (thiserror)
├── schema.rs         — Verdict, Severity, Mode, AgentName, Finding, AgentOutput
├── validate.rs       — Validator with ValidationLimits, zero-width Unicode stripping
├── consensus.rs      — ConsensusEngine: weighted scoring, epsilon-aware classification
├── reporting.rs      — ReportFormatter (52-char ASCII banner), MagiReport
├── provider.rs       — LlmProvider async trait (Send+Sync), RetryProvider
├── prompts.rs        — 3 submodules loading 9 system prompt .md files via include_str!
├── agent.rs          — Agent struct, AgentFactory with per-agent/per-mode overrides
├── orchestrator.rs   — Magi struct + MagiBuilder, analyze() via concurrent dispatch
├── prelude.rs        — Re-exports of all public types
└── providers/
    ├── claude.rs     — ClaudeProvider (HTTP, feature: claude-api)
    └── claude_cli.rs — ClaudeCliProvider (subprocess, feature: claude-cli)
```

### 4.2 Dependency Flow

```
error         (foundation — no internal deps)
  ↓
schema        (domain types)
  ↓
validate      (field validation with regex zero-width stripping)
  ↓
consensus     (weighted scoring, classification, finding dedup)
  ↓
reporting     (ASCII banner + markdown report generation)

provider      (LlmProvider trait, CompletionConfig, RetryProvider)
  ↓
agent         (Agent struct, AgentFactory, system prompt loading)
  ↓
orchestrator  (Magi, MagiBuilder — composes everything)
```

Providers are feature-gated and independent of the core dependency chain.

### 4.3 Execution Pipeline

```
User input
  │
  ▼
Magi::analyze() — validates input size
  │
  ▼
AgentFactory::create_agents() — 3 agents with mode-specific prompts
  │
  ├──────────────────┬──────────────────┐
  ▼                  ▼                  ▼
Melchior           Balthasar          Caspar
(tokio::spawn)     (tokio::spawn)     (tokio::spawn)
  │                  │                  │
  ▼                  ▼                  ▼
parse_agent_response() — strip code fences, extract JSON
  │                  │                  │
  ▼                  ▼                  ▼
Validator::validate() — confidence, text lengths, findings
  │
  ▼
ConsensusEngine::determine() — scoring + dedup + dissent
  │
  ▼
ReportFormatter::format_report() — ASCII banner + markdown
  │
  ▼
MagiReport { agents, consensus, banner, report, degraded, failed_agents }
```

### 4.4 Concurrency Model

Agents are launched as independent `tokio::spawn` tasks with per-agent timeout via `tokio::time::timeout`. An `AbortGuard` RAII struct holds `AbortHandle`s for all spawned tasks — if the `analyze()` future is dropped (e.g., caller timeout), the guard aborts every running task, preventing wasted LLM API quota.

Key properties:

- **Parallel execution**: Total time equals the slowest agent, not the sum.
- **Agent identity preserved**: Each `JoinHandle` is stored alongside its `AgentName`, ensuring panicked tasks are correctly attributed.
- **Graceful degradation**: If one agent fails, synthesis proceeds with two (flagged as degraded). If fewer than two succeed, `MagiError::InsufficientAgents` is returned.
- **Failure diagnostics**: `MagiReport.failed_agents` is a `BTreeMap<AgentName, String>` containing the specific failure reason for each failed agent.

---

## 5. Data Schema and Consensus Protocol

### 5.1 Agent Output Schema

Each agent responds with a JSON object (deserialized as `AgentOutput`):

```json
{
  "agent": "melchior | balthasar | caspar",
  "verdict": "approve | reject | conditional",
  "confidence": 0.85,
  "summary": "One-line verdict summary",
  "reasoning": "Detailed analysis (2-5 paragraphs)",
  "findings": [
    { "severity": "critical | warning | info", "title": "Short title", "detail": "Explanation" }
  ],
  "recommendation": "What this agent recommends"
}
```

Key fields:

- **verdict**: The binary vote (`conditional` counts as approve for majority but generates conditions in the report).
- **confidence**: Agent certainty in its own verdict (0.0-1.0). Validated to reject NaN/Infinity.
- **findings**: Atomic units of analysis — the consensus engine deduplicates and merges by case-insensitive title.

### 5.2 Voting Rules

Weight-based scoring in `ConsensusEngine`:

```
VERDICT_WEIGHT = { approve: +1.0, conditional: +0.5, reject: -1.0 }
score = sum(weight) / num_agents
```

| Score | Condition           | Consensus            |
|-------|---------------------|----------------------|
| 1.0   | Unanimous approve  | **STRONG GO**        |
| > 0   | Has conditionals   | **GO WITH CAVEATS**  |
| > 0   | No conditionals    | **GO (N-M)**         |
| 0     | Tie                | **HOLD -- TIE**      |
| < 0   | Mixed              | **HOLD (N-M)**       |
| -1.0  | Unanimous reject   | **STRONG NO-GO**     |

In degraded mode (2/3 agents), STRONG labels are capped to their regular counterparts.

### 5.3 Confidence Formula

```
base_confidence = sum(majority_side_confidences) / total_agent_count
weight_factor = (|score| + 1) / 2
confidence = clamp(base_confidence * weight_factor, 0.0, 1.0)
```

Key properties:

- **Penalizes non-unanimity**: `base_confidence` divides by total agent count, not majority count. A dissenting agent dilutes confidence even though it's not on the majority side.
- **Symmetric**: Unanimous reject at 0.9 confidence produces system confidence of 0.9, matching unanimous approve.
- **Tie-aware**: At `score = 0`, `weight_factor = 0.5`, halving confidence — a tie genuinely represents lower certainty.
- **Clamped and rounded**: Final confidence is clamped to [0.0, 1.0] and rounded to 2 decimal places.

### 5.4 Findings Deduplication

The consensus engine merges findings from all agents:

1. **Deduplication by title**: Case-insensitive matching with zero-width Unicode characters stripped via regex.
2. **Severity escalation**: When the same finding has different severities across agents, the highest wins (Critical > Warning > Info).
3. **Sorting**: Final findings sorted by severity (Critical first).
4. **Source tracking**: Each deduplicated finding lists all contributing agents in its `sources` array.

---

## 6. Modes of Operation

### 6.1 Code Review

**Trigger:** Code, diffs, or source files to evaluate.

- Melchior reviews correctness and algorithmic efficiency.
- Balthasar evaluates readability and maintainability.
- Caspar searches for edge cases and vulnerabilities.

### 6.2 Design

**Trigger:** Architecture, approach selection, or solution design.

- Melchior evaluates theoretical soundness and formal properties.
- Balthasar estimates implementation cost, migration burden, and operational overhead.
- Caspar identifies failure points, scaling cliffs, and hidden coupling.

### 6.3 Analysis

**Trigger:** General problem analysis, debugging, trade-offs, or technical decisions. Default mode when the input doesn't clearly fit code-review or design.

---

## 7. Design Philosophy

### 7.1 Dissent is a Feature

The system is designed so that agents **disagree**. If all three always agree, the system is failing — probably the system prompts are not sufficiently differentiated, or the problem is trivial and doesn't need MAGI.

The system's value emerges precisely when Caspar rejects something that Melchior and Balthasar approved. That rejection forces the user to consider risks they would otherwise ignore.

### 7.2 Adversarial by Design

Caspar exists to be adversarial. Its system prompt explicitly instructs it to find flaws. This is not a weakness of the system — it is the mechanism that prevents groupthink. In the series, when all three MAGI vote the same way, it is usually a sign that something is very wrong (like an external hack forcing unanimity).

### 7.3 Proportionality

Not everything needs MAGI. A trivial bug, a typo, or a question with an obvious answer does not justify three agents. MAGI adds value for decisions with:

- **Genuine uncertainty** about the best path.
- **Significant consequences** if the decision is wrong.
- **Multiple stakeholders** with different priorities.
- **Genuine trade-offs** where there is no objectively superior answer.

### 7.4 LLM-Agnostic Design

magi-core's `LlmProvider` trait abstracts over any LLM backend. The library does not depend on any specific model or API — Claude, Gemini, OpenAI, or local models can all serve as the underlying engine. The built-in Claude providers are feature-gated and optional.

This means the same consensus engine, validation, and reporting pipeline works regardless of which LLM powers the agents — or even if different agents use different models.

---

## 8. Evangelion Correspondence Table

| Element in Evangelion | Equivalent in magi-core |
|----------------------|--------------------------|
| NERV's MAGI System | `Magi` orchestrator struct |
| Dr. Naoko Akagi (creator) | LLM backend (base model for all 3 agents) |
| MELCHIOR-1 (scientist) | `AgentName::Melchior` — technical rigor and correctness |
| BALTHASAR-2 (mother) | `AgentName::Balthasar` — pragmatism and team protection |
| CASPAR-3 (woman) | `AgentName::Caspar` — adversarial instinct and risk detection |
| 2-of-3 voting | `ConsensusEngine` with weight-based majority rules |
| Personality transplant | System prompts (9 markdown files, 3 agents x 3 modes) |
| Terminal Dogma | `MagiBuilder` (hidden configuration depth) |
| AT Field | Agent independence (parallel execution, no shared context) |
| Pribnow Box | `Validator` (schema validation — containment layer) |
| Entry Plug | `LlmProvider` trait (the interface connecting the model to the agent) |

---

## 9. Relationship to the MAGI Python Plugin

magi-core is a **Rust port and generalization** of the [MAGI Python plugin](https://github.com/BolivarTech/magi) for Claude Code. The Python plugin implements the MAGI system as a Claude Code skill with a synthesis pipeline (`validate.py`, `consensus.py`, `reporting.py`) and a CLI orchestrator (`run_magi.py`).

magi-core preserves the same consensus algorithm, confidence formula, and findings deduplication logic, but repackages them as a general-purpose Rust library with:

- **Async trait-based provider abstraction** (`LlmProvider`) instead of hardcoded `claude -p` subprocess calls.
- **Feature-gated providers** for Claude HTTP API and CLI, with the door open for Gemini, OpenAI, and local models.
- **Compile-time embedded prompts** via `include_str!` instead of runtime file loading.
- **Builder pattern** (`MagiBuilder`) for flexible configuration.
- **Type-safe domain model** with Rust enums, serde serialization, and comprehensive validation.

---

*Technical reference document for magi-core v0.1.0.*
*The MAGI concept originates from Neon Genesis Evangelion (Hideaki Anno, Gainax, 1995).*
*The implementation as a Rust library is a creative adaptation for LLM-agnostic multi-perspective analysis.*
