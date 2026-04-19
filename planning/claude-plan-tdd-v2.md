# Plan TDD estricto — MAGI-Core v0.2.0

**Objetivo:** alinear `magi-core` (Rust v0.1.2) con la implementacion de referencia `MAGI` (Python v2.1.3) en `D:\jbolivarg\PythonProjects\MAGI`.

**Contrato de equivalencia (justificado):** se exige **equivalencia byte-a-byte** en la salida de `ReportFormatter::format_report()` (banner + secciones) para que consumidores downstream (herramientas de diff, tests de snapshot, integraciones CI que parsean el reporte) sean intercambiables entre ambas implementaciones. La algoritmica (consenso, dedup, clasificacion) debe ser semanticamente equivalente — dado el mismo input se produce el mismo veredicto, confianza y orden de findings. Los artefactos JSON deben tener el mismo shape y valores.

**Contrato de formato numerico (normativo, aplica a todo el plan):**
- `confidence` en JSON: redondeado a 2 decimales, `round(v * 100) / 100`. Un valor `0.9` se serializa como `0.9`, no `0.90`.
- Porcentajes en banner: entero, `round(confidence * 100)`, sin padding, sin decimales (`90%`, no `090%` ni `90.0%`).
- Split `(N-M)`: enteros sin padding, separador `-` (ascii hyphen, no en-dash).
- Score raw en JSON: `f64` sin redondear.

**Version objetivo:** `magi-core v0.2.0` — cambia contrato publico (prompts, dedup key, reporte), justifica bump minor.

**Soft-landing de breaking changes:**
1. Publicar `v0.2.0-rc.1` a crates.io tras completar S01..S10, S12, S13, ventana de feedback **48–72h** (reducida desde 7 dias por ser ventana interna y no publica). Si no hay consumer explicito nombrado que valide rc.1, se omite y se tagea `v0.2.0` directo — la ventana existe solo si hay feedback real esperado.
2. Para APIs que se renombran o mutan firma (`stripped_title`), mantener el symbolo anterior con `#[deprecated(since = "0.2.0", note = "...")]` delegando al nuevo. Eliminar solo en `v0.3.0`.
3. Migration guide (`docs/migration-v0.2.md`) publicado simultaneamente con rc.1.

**Scope explicito:** este plan cubre `v0.2.0` = "algorithmic + report equivalence". El cambio arquitectural de prompts (S11 en rondas MAGI previas) se trata aparte en `planning/claude-plan-tdd-v0.3-prompts.md`.

**Rollback strategy:** `v0.1.2` esta tageado y publicado en crates.io. Cada bloque (1–5) se commitea en secuencia sobre `main`. Si tras un bloque se detecta bug bloqueante:
- Opcion A (rollback rapido): `git revert` del range de commits del bloque; no se crea yank.
- Opcion B (yank + rollback): si ya se publico rc, `cargo yank v0.2.0-rc.N`, revert, nueva rc.
- No se usan feature-gates de tipo `#[cfg(feature = "v2-consensus")]` — añaden deuda duradera para mitigar riesgo de ~5 dias.

**Rehearsal del rollback (obligatorio antes del primer merge a main):**
Antes de commitear cualquier cambio de v0.2.0 a `main`, ejecutar un dry-run del rollback en una branch de prueba:
1. `git checkout -b rehearsal/rollback-test main`
2. Aplicar un commit trivial simulando v0.2.0 change
3. `git log --oneline v0.1.2..HEAD` — confirmar el range
4. `git revert --no-commit v0.1.2..HEAD` — confirmar que el revert es clean
5. `cargo build && cargo nextest run` — verificar que post-revert todo compila y pasa
6. `git checkout main && git branch -D rehearsal/rollback-test`

Esto asegura que el "plan de rollback" no es teorico: se sabe que funciona antes de necesitarlo.

**Fixture generation (contrato de reproducibilidad):**
- Script `tests/fixtures/generate.py` comiteado al repo. Pasos:
  1. Pin explicito de Python MAGI a commit `MAGI_REF_SHA` (variable en el script, default = SHA de `v2.1.3`).
  2. Genera `clean_title_corpus.txt` (S02), `dedup_key_corpus.jsonl` (S03), `fit_content_vectors.jsonl` (S10).
- Tests cargan los archivos, no regeneran en runtime.
- Politica de regeneracion: re-run solo tras bump explicito de `MAGI_REF_SHA` en un commit dedicado (`chore: bump MAGI reference fixtures to <sha>`).
- Cabecera de cada fixture incluye `# Generated from MAGI@<sha> on <date>` para auditoria.
- **Integridad verificable:** cada fixture tiene un `.sha256` hermano (e.g., `clean_title_corpus.txt.sha256`) commiteado. CI hace `sha256sum -c` antes de correr tests — si el fixture fue modificado manualmente sin regenerar, el build falla.
- **Friccion del Python dep:** aceptada en v0.2.0 (Python ya es requerido por `run-tests.py` y `tdd-guard`). **Enhancement v0.3.0:** portar `generate.py` → `tests/fixtures/src/generate.rs` (binary crate en el workspace) para eliminar dependencia en Python runtime — usar `pyo3` para ejecutar el MAGI Python original como biblioteca, cacheando outputs en JSON. Tracked en `ROADMAP.md` como v0.3.0 candidate.

**Metodologia:** SBTDD con ciclo Red-Green-Refactor estricto. **Commits agrupados por seccion** (no por fase) para reducir round-trips de aprobacion: un commit test: + un commit feat:/fix: + un commit refactor: opcional por seccion. **Refactor vacio se elide** — si no hay trabajo real de limpieza, se omite el commit. Verificacion completa entre secciones (tests, clippy, fmt, release, docs, audit).

**Contrato TDD estricto (normativo, aplica a todas las secciones):**

1. **Red genuino:** tras el commit `test:`, ejecutar `python run-tests.py` y confirmar que **al menos un test del nuevo/modificado conjunto falla** (compile error o assertion failure). Si todos pasan, el Red no es genuino — ajustar antes de Green.
2. **Green minimo:** el commit `feat:`/`fix:` contiene **solo el codigo necesario para que los tests pasen**. Nada de funcionalidad extra, refactor preventivo, ni abstracciones no pedidas por algun test.
3. **No produccion sin test rojo previo:** TDD-Guard (hook PreToolUse) intercepta cada escritura en codigo de produccion y bloquea si no hay un test rojo previo en la fase Red. Esto es enforcement de runtime, no discrecional.
4. **Inversion de assertion cuenta como Red:** modificar un test existente para esperar el nuevo comportamiento (p. ej. `assert_eq!(x, "new")` donde antes era `"old"`) pone el test en rojo y es un Red valido.
5. **Eliminacion de tests nunca en Red:** los tests obsoletos se eliminan solo en la fase Refactor, despues de verificar que el nuevo comportamiento esta implementado y green. Eliminar en Red "oculta" lo que se rompe.
6. **Refactor no cambia comportamiento:** cero funcionalidad nueva, cero tests modificados en assertions, cero nuevas ramas. Solo extraccion, rename, deduplicacion, doc.
7. **Verificacion post-Red obligatoria:** cada seccion debe documentar en el commit message de Green cuantos tests fallaron tras el Red y cuantos pasan tras el Green.

**Estimacion de esfuerzo:** ~60–75 commits reales (no 42), considerando que secciones como S11 implican multiples commits internos y que los tests listados totalizan ~95 casos nuevos (no 50). Rango estimado: 3–5 dias de trabajo focalizado.

---

## 1. Gap analysis — resumen ejecutivo

### 1.1 Divergencias criticas (rompen equivalencia)

| # | Gap | Python (v2.1.3) | Rust (v0.1.2) | Prioridad |
|---|-----|-----------------|---------------|-----------|
| G01 | Alias `opus` | `claude-opus-4-7` | `claude-opus-4-6` | Alta |
| G02 | Prompts por agente | **3 archivos** (mode-agnosticos, inyeccion por payload) | **9 archivos** (3 agentes x 3 modos, compile-time) | Critica |
| G03 | Clave de dedup de findings | `clean_title` + `NFKC` + `casefold` | `stripped_title` + `split_whitespace` + `to_lowercase` | Critica |
| G04 | `clean_title` | strip zero-width + reemplazar control-whitespace por espacio + trim | solo strip zero-width | Critica |
| G05 | Fuente de `conditions` | `a.summary` | `a.recommendation` | Critica |
| G06 | Etiqueta `GO WITH CAVEATS` con split | `GO WITH CAVEATS (2-1)` | `GO WITH CAVEATS` (sin split) | Critica |
| G07 | Seccion `## Consensus Summary` en markdown | eliminada en 2.1.x | presente | Critica |
| G08 | Dissent: solo summary | una linea por disidente, summary unicamente | summary + parrafo de reasoning | Critica |
| G09 | `format_findings` incluye detail en output | NO incluye detail | SI incluye detail como parrafo indentado | Critica |
| G10 | Prefijo de agente en `majority_summary` | `"Melchior: summary \| Balthasar: summary"` | `"summary \| summary"` (sin prefijo) | Critica |
| G11 | Alineacion de columnas en banner | labels padded a `max_label_len`, verdict protegido via `preserve_suffix` | cada linea renderizada sin alineacion de columnas | Critica |

### 1.2 Divergencias importantes (comportamiento)

| # | Gap | Python | Rust | Prioridad |
|---|-----|--------|------|-----------|
| G12 | Limite por defecto de input size | `10 MB` (`10 * 1024 * 1024`) | `1 MB` (`1_048_576`) | Media |
| G13 | Validator reemplaza title in-place con version limpia | si (`f["title"] = clean_title(f["title"])`) | no (solo valida longitud) | Media |
| G14 | Parser robusto de salida de Claude (`_extract_text`) | maneja `{"result"}`, `{"content": [{"type": "text"}]}`, string plano | solo maneja envelope `{"is_error", "result"}` en `ClaudeCliProvider` | Media |
| G15 | Zero-width strip aplicado tambien a `detail` | no lo aplica al detail tampoco | no lo aplica | Baja (ambos igual) |

### 1.3 Mejoras informativas (no son gaps de equivalencia)

- Python tiene `status_display` (UI en terminal). No portable a libreria Rust, se descarta.
- Python tiene `stderr_shim` y `subprocess_utils` para Windows. Especifico de su modelo subprocess; `ClaudeCliProvider` ya maneja su propio timeout via tokio.
- Python tiene `temp_dirs` LRU cleanup. Responsabilidad del consumidor, no de la libreria.
- Init banner en Rust (`format_init_banner`): Python no lo tiene. **No se elimina** — extension util y compatible.

---

## 2. Plan por secciones

Cada seccion sigue el ciclo **Red → Green → Refactor** con verificacion obligatoria. Los nombres de tests siguen el patron BDD del proyecto (`test_<behavior>_<given_when>`).

### Seccion S01 — Correccion del alias `opus` (G01)

**Archivo objetivo:** `src/provider.rs`

**Red (commit `test:`)**
- `test_resolve_claude_alias_opus_returns_claude_opus_4_7`
- `test_resolve_claude_alias_sonnet_returns_claude_sonnet_4_6`
- `test_resolve_claude_alias_haiku_returns_claude_haiku_4_5_20251001`
- Modificar el test existente de opus si contradice el nuevo valor.

**Green (commit `fix:`)**
- Cambiar literal `"claude-opus-4-6"` → `"claude-opus-4-7"` en `resolve_claude_alias`.
- Actualizar docstring rustdoc.

**Refactor (commit `refactor:`)**
- Ninguno (cambio de un literal).

**Verificacion:** `python run-tests.py && cargo clippy --tests -- -D warnings && cargo fmt --check && cargo build --release && cargo doc --no-deps && cargo audit`

---

### Seccion S02 — Funcion `clean_title` unificada (G04)

**Archivo objetivo:** `src/validate.rs` (nueva funcion publica); `src/schema.rs` (deprecar `stripped_title` o reimplementarla).

**Spec literal (mirror de Python):**
```
clean_title(input: &str) -> String
  1. Aplicar CONTROL_WHITESPACE_RE = r"[\t\n\v\f\r\x85]" sustituyendo por ' '
     (coincide con Python _CONTROL_WHITESPACE_RE).
  2. Aplicar INVISIBLE_AND_SEPARATOR_RE = r"[\u{200b}-\u{200f}\u{2028}-\u{202f}\u{2060}-\u{206f}\u{feff}\u{00ad}]"
     eliminando (coincide con Python _ZERO_WIDTH_RE, nombre heredado).
  3. .trim() — elimina leading/trailing whitespace ASCII + Unicode
     (equivalente a str.strip() de Python que usa propiedad White_Space).
```

**Nota sobre el nombre `INVISIBLE_AND_SEPARATOR_RE` (rename desde `ZERO_WIDTH_RE`):**
El rango no es estrictamente "zero-width". Incluye:
- **Zero-width reales** (Cf): U+200B (ZWSP), U+200C (ZWNJ), U+200D (ZWJ), U+FEFF (BOM), U+00AD (soft hyphen).
- **Bidi marks** (Cf): U+200E-U+200F (LRM/RLM), U+202A-U+202E (LRE/RLE/PDF/LRO/RLO).
- **Separators que ocupan ancho** (Zl/Zs/Zp): U+2028 (LINE SEPARATOR), U+2029 (PARAGRAPH SEPARATOR), U+202F (NARROW NO-BREAK SPACE) — estos **no son zero-width**, son espaciadores visibles.
- **Controles extendidos** (Cf): U+2060-U+206F (WORD JOINER, invisible times/separator/plus, interlinear annotations, etc.).

Se conserva exactamente el set de Python (fidelidad byte-a-byte) aunque algunos codepoints violen el nombre literal. El rename del constante en Rust documenta esta realidad; la regex es identica a Python.

**Nota (diferencia Python-Rust documentada):** Python `str.strip()` elimina caracteres con propiedad `White_Space` (incluye espacios reemplazados que ya son ASCII). Rust `str::trim()` usa `char::is_whitespace` que cubre el mismo set. **No** colapsa whitespace interior — tras el paso 1, un input `"foo\t\tbar"` queda como `"foo  bar"` (dos espacios). Esto es **comportamiento intencional de Python**; el colapso de whitespace interior solo ocurre implicitamente en la dedup key de consensus (split_whitespace + join) — ver S03.

**Red (commit `test:`)**
- `test_clean_title_replaces_tab_with_space`
- `test_clean_title_replaces_newline_with_space`
- `test_clean_title_replaces_vertical_tab_with_space`
- `test_clean_title_replaces_carriage_return_with_space` (verifica `\r`)
- `test_clean_title_replaces_nel_u0085_with_space`
- `test_clean_title_strips_zero_width_space_u200b`
- `test_clean_title_strips_zwnj_u200c` (no joiner)
- `test_clean_title_strips_zwj_u200d` (joiner)
- `test_clean_title_strips_lrm_rlm_u200e_u200f` (bidi marks basicos)
- `test_clean_title_strips_line_separator_u2028` (Zl)
- `test_clean_title_strips_paragraph_separator_u2029` (Zp)
- `test_clean_title_strips_narrow_nbsp_u202f` (dentro del rango U+2028..U+202F)
- `test_clean_title_strips_bidi_override_u202a_through_u202e` (6 codepoints, uno por uno)
- `test_clean_title_strips_word_joiner_u2060` (rango U+2060..U+206F)
- `test_clean_title_strips_bom_ufeff`
- `test_clean_title_strips_soft_hyphen_u00ad`
- `test_clean_title_trims_leading_trailing_spaces`
- `test_clean_title_trims_leading_trailing_tabs_after_replacement`
- `test_clean_title_preserves_interior_single_spaces`
- `test_clean_title_does_not_collapse_double_spaces_interior` (input `"foo  bar"` → output `"foo  bar"`)
- `test_clean_title_preserves_unicode_letters` (e.g., `"café"` sobrevive)
- `test_clean_title_empty_string_returns_empty`
- `test_clean_title_all_whitespace_returns_empty`
- `test_clean_title_is_idempotent` (propiedad: `clean_title(clean_title(x)) == clean_title(x)` para varios x representativos — garantiza que aplicar dos veces no es un bug)

**Corpus test (commit adicional si se desea):**
- `test_clean_title_matches_python_corpus` — cargar fixtures `tests/fixtures/clean_title_corpus.txt` con pares `input\tpython_output` generados con un script one-shot desde la Python MAGI y verificar equivalencia exacta.

**Green (commit `feat:`)**
- Declarar dos constantes regex usando `std::sync::LazyLock` (estable desde Rust 1.80, MSRV 1.91 lo soporta — verificado).
  - Usar el crate `regex` (ya esta en `Cargo.toml` como dep directa).
- Implementar `pub fn clean_title(input: &str) -> String` con las tres transformaciones en orden exacto.
- Reexportar desde `prelude.rs`.

**Refactor (commit `refactor:`, opcional)**
- Deprecar `Finding::stripped_title()` con `#[deprecated(since = "0.2.0", note = "use clean_title")]` reimplementandolo como wrapper.
- Actualizar docstrings de `Finding.title`.

---

### Seccion S03 — Clave de dedup con NFKC + casefold real (G03)

**Archivo objetivo:** `src/consensus.rs` (funcion `deduplicate_findings`), agregar dependencias `unicode-normalization` y `caseless`.

**Decision de casefold (resuelta, no pendiente):** se adopta el crate `caseless` v0.2 (sin releases frecuentes pero sin unsafe, probado, ~3KB) para tener **full Unicode casefolding** equivalente a Python `str.casefold()`. No usar `to_lowercase()` — `ß.to_lowercase() = "ß"` mientras que `ß.casefold() = "ss"`, y el contrato de equivalencia lo requiere. **No se usan tests `#[ignore]`** en el plan final.

**Spec literal (mirror de Python `_dedup_key`):**
```
dedup_key(title: &str) -> String
  1. clean_title(title)                              // S02
  2. unicode_normalization::UnicodeNormalization::nfkc(...)
  3. caseless::default_case_fold_str(...)
```

**Colapso de whitespace interior (clarificacion):** Python `clean_title` NO colapsa whitespace interior (mantiene `"foo  bar"` con doble espacio tras reemplazo de tabs). La dedup key de Python aplica NFKC + casefold directamente, tampoco colapsa. El comportamiento actual de Rust (`split_whitespace + join`) **es una divergencia** con Python. Se **elimina** esa normalizacion en el paso de `dedup_key`, coincidiendo con Python. Tests cubriran ambos lados.

**Pin de dependencias (politica del plan):**
```toml
unicode-normalization = "~0.1.24"   # tilde: acepta 0.1.x (x >= 24), no 0.2+
caseless = "~0.2.2"                 # tilde: acepta 0.2.x (x >= 2), no 0.3+
```
**Justificacion del tilde vs `=` estricto:** el pin estricto genera deuda de mantenimiento (fuerza update manual del lockfile en cada patch upstream aunque sea semver-compatible) y fragmenta el DAG de deps de downstream (si otro crate trae `unicode-normalization 0.1.25`, resolver falla). El tilde equilibra reproducibilidad con compatibilidad semver-patch.

**Nota sobre `caseless`:** el crate esta estable pero con pocos releases recientes. Supply-chain signal aceptable por (1) funcion pura sin deps transitivas peligrosas, (2) ausencia de `unsafe`, (3) API minima (`default_case_fold_str` es una sola funcion).

**Plan de fallback vendored (contingencia supply-chain):**
Si `caseless` es retirado de crates.io, comprometido, o deja de compilar con MSRV futuro:
1. Vendorizar el crate como `vendor/caseless/` en el repo, con licencia intacta (MIT-0).
2. Agregar a `Cargo.toml`: `caseless = { path = "vendor/caseless" }`.
3. Nota en CHANGELOG: "Vendored caseless 0.2.2 due to <reason>; re-evaluate upstream in N+1."
4. El contenido del crate es ~500 lineas de tablas Unicode + logica de casefold; auditable en <1h.

**Alternativas ponderadas** (documentadas en ADR si se migra):
- `icu_casemap` (ICU4X): ~2MB footprint, activo, overkill para un solo uso.
- Implementacion custom usando tablas de `unicode-case-mapping`: viable si `caseless` + `icu_casemap` ambas son inaceptables.

Re-evaluar en cada release con `cargo audit` y `cargo outdated`.

**Red (commit `test:`)**
- `test_dedup_key_nfkc_collapses_fullwidth_digits` (`"ＡＢＣ"` == `"abc"` tras NFKC+casefold)
- `test_dedup_key_nfkc_collapses_combining_accents` (`"café"` precompuesto [U+00E9] == `"café"` con combining [U+0065 U+0301])
- `test_dedup_key_casefold_sharp_s_equals_double_s` (`"ß"` == `"ss"`) — **sin `#[ignore]`**
- `test_dedup_key_casefold_greek_sigma_variants` (`"Σ"` == `"σ"`) — nota: `Σ → σ` via casefold; el caso `ς → σ` es **ambiguo en default casefold** (Unicode final-sigma folding es "locale-sensitive" y `caseless::default_case_fold_str` puede no aplicarlo). **Accion:** antes de escribir el test, ejecutar tanto `python -c "print('ς'.casefold())"` como `caseless::default_case_fold_str("ς")` y comparar; si difieren, testear solo `Σ == σ` (caso no-ambiguo) y documentar en rustdoc que `ς` puede no colapsar con `σ` — divergencia aceptada vs Python si se confirma.
- `test_dedup_key_casefold_turkish_dotted_i` (no se trata especial: Unicode default casefold, no locale-aware)
- `test_dedup_key_preserves_interior_whitespace` (confirma `"foo  bar"` != `"foo bar"`, alineado con Python)
- `test_dedup_merges_fullwidth_and_ascii_titles`
- `test_dedup_key_matches_python_corpus` (fixtures desde corpus Python, misma idea que S02)

**Contrato de ordering (CRITICO — no usar `HashMap`):**
El orden de presentacion de findings tras dedup debe ser: **`first_seen_by_agent_iteration_order`**, luego **sorted by severity DESC (Critical → Info)**. El "first seen" se define como el primer agente en aparecer en el slice `&[AgentOutput]` que reporto un finding cuya `dedup_key` coincide.

**Tests de ordering (regression):**
- `test_dedup_first_seen_order_preserved_when_melchior_reports_first`:
  ```rust
  // Melchior reporta "Issue A", Balthasar reporta "issue a"
  // Despues de dedup, sources = [Melchior, Balthasar] en ese orden
  // title = "Issue A" (forma original de Melchior)
  ```
- `test_dedup_first_seen_order_preserved_when_balthasar_reports_first`:
  ```rust
  // Mismo test pero con agentes invertidos en el slice
  // sources = [Balthasar, Melchior]; title = "issue a"
  ```
- `test_dedup_ordering_stable_across_equal_severity` — dos findings de igual severity: el primero en ver gana la posicion de salida.
- `test_dedup_uses_indexmap_or_vec_not_hashmap` — test inspeccion/compilacion (annotacion `#[deny(clippy::...)]` o grep test) que asegura que el data structure en `deduplicate_findings` preserva insertion order.

**Green ajustado (decision estructural):**
Usar `Vec<(String, GroupState)>` con busqueda lineal en vez de `HashMap<String, ...>` — preserva orden de insercion sin crates adicionales.

**Complejidad (documentada intencionalmente):** insercion de m findings totales hace **O(m²)** comparaciones (linear scan por cada insercion). Para m ≤ 300 (maximo teorico: 100 findings × 3 agents), m² = 90000 comparaciones de strings cortas, ejecucion < 1ms en hardware moderno. **Trade-off aceptado:** simplicidad vs. `indexmap` (que seria O(m) pero agrega dep). Re-evaluar si el limite de `max_findings` se eleva > 500 en el futuro.

Comentario obligatorio en el codigo:
```rust
// Intentional O(m²) — preserves insertion order without adding indexmap.
// For m ≤ 300 (capped by ValidationLimits::max_findings × 3 agents), cost
// is ~90k string comparisons on short strings, <1ms in practice. Switch to
// `indexmap` if max_findings raises above 500.
```

**Green (commit `feat:`)**
- Agregar al `Cargo.toml`:
  ```toml
  unicode-normalization = "=0.1.24"
  caseless = "=0.2.2"
  ```
- Implementar funcion privada `dedup_key(title: &str) -> String` en `consensus.rs`:
  ```rust
  fn dedup_key(title: &str) -> String {
      use unicode_normalization::UnicodeNormalization;
      caseless::default_case_fold_str(
          &clean_title(title).nfkc().collect::<String>()
      )
  }
  ```
- Reemplazar la logica actual que usa `split_whitespace + join` — **eliminar la normalizacion de whitespace interior** para coincidir con Python.
- En el loop de dedup, usar `dedup_key(&finding.title)`.

**Refactor (commit `refactor:`, opcional)**
- Extraer `dedup_key` a `validate.rs` como utility compartida si se usa en otras partes.
- Actualizar doc del metodo para describir el orden de transformaciones y la diferencia semantica con la implementacion v0.1.x.

---

### Seccion S04 — `conditions` desde `summary`, no `recommendation` (G05)

**Archivo objetivo:** `src/consensus.rs`.

**Red (commit `test:`)**
- Modificar `test_conditions_extracted_from_conditional_agents` invirtiendo la assertion — esto lo pone en rojo inmediatamente sobre v0.1.2:
  ```rust
  // Antes: assert_eq!(result.conditions[0].condition, "Balthasar recommendation");
  // Despues: assert_eq!(result.conditions[0].condition, "Balthasar summary");
  ```
- `test_conditions_use_summary_field_not_recommendation_field` — test nuevo que verifica explicitamente el nuevo contrato; falla en v0.1.2.
- `test_conditions_are_distinct_from_recommendations_section` — test nuevo que usa summary≠recommendation en el setup; falla si ambos valores coinciden.
- **Precondicion de TDD-Guard:** verificar que `python run-tests.py` falla al menos en estos 3 tests antes de proceder a Green.

**Green (commit `fix:`)**
- En `ConsensusEngine::determine`, cambiar:
  ```rust
  condition: a.recommendation.clone(),
  ```
  por:
  ```rust
  condition: a.summary.clone(),
  ```
- Actualizar docstring de `Condition` struct.

**Refactor (commit `refactor:`)**
- Revisar que los tests de reporting que esperan `condition` reflejen summary.
- Eliminar tests que quedaron redundantes tras la inversion (si los hay).

---

### Seccion S05 — Etiqueta `GO WITH CAVEATS` con split count (G06)

**Archivo objetivo:** `src/consensus.rs` (funcion `classify`).

**Spec normativa (alineada con Python `_format_consensus_label`):**
- Approve+Conditional cuentan ambos en el lado "go"; solo Reject esta en "no-go".
- Split se escribe como `(majority-minority)` donde majority es el lado del consensus_verdict.
- Degraded (< 3 agentes) **solo** recapea STRONG GO/NO-GO, no altera `GO WITH CAVEATS`.

**Red (commit `test:`)**
- Modificar `test_approve_conditional_reject_produces_go_with_caveats`:
  ```rust
  assert_eq!(result.consensus, "GO WITH CAVEATS (2-1)");
  ```
- `test_go_with_caveats_three_conditionals_unanimous` → `GO WITH CAVEATS (3-0)` (no STRONG GO, porque hay condicionales)
- `test_go_with_caveats_two_conditionals_one_approve` → `GO WITH CAVEATS (3-0)` (todos en go-side)
- `test_go_with_caveats_two_conditionals_one_reject` → `GO WITH CAVEATS (2-1)`
- `test_go_with_caveats_degraded_two_conditionals` → `GO WITH CAVEATS (2-0)` (2 agentes, no cap, resto del flag degraded se mantiene)
- `test_go_with_caveats_degraded_one_conditional_one_approve` → `GO WITH CAVEATS (2-0)`
- `test_degraded_one_conditional_one_reject_produces_hold_1_1` → expected `HOLD (1-1)` (score = (0.5 + -1)/2 = -0.25, negativo, NO es tie; el conditional cuenta en go-side pero reject gana). **Nota:** el nombre del test fue corregido tras verificar la expected outcome; el nombre anterior `test_go_with_caveats_degraded_one_conditional_one_reject` contradecia el resultado y se elimino.
- **Test analitico de boundaries:** `test_score_just_above_epsilon_classifies_as_go`, `test_score_just_below_epsilon_classifies_as_hold` — usar agentes constructores con weights que produzcan score = ±1.5e-9.

**Green (commit `fix:`)**
- En `classify()`, cambiar:
  ```rust
  } else if score > epsilon && has_conditional {
      ("GO WITH CAVEATS".to_string(), Verdict::Approve)
  ```
  por:
  ```rust
  } else if score > epsilon && has_conditional {
      (format!("GO WITH CAVEATS ({}-{})", approve_count, reject_count), Verdict::Approve)
  ```

**Refactor (commit `refactor:`, opcional)**
- Extraer una helper `split_label(approve, reject) -> String` si la repeticion lo amerita (probable reuso con S10).

---

### Seccion S06 — Eliminar seccion `## Consensus Summary` del reporte (G07)

**Archivo objetivo:** `src/reporting.rs`.

**Red (commit `test:`)**
- `test_report_does_not_contain_consensus_summary_heading`:
  ```rust
  assert!(!report.contains("## Consensus Summary"));
  ```
- `test_report_section_order_banner_then_findings_or_dissent_or_conditions_or_actions`
- **Modificar** tests existentes que asserteaban la **presencia** de `## Consensus Summary` para invertir su assertion (ahora esperan ausencia). Esto los pone en rojo inmediatamente antes del Green. **No se eliminan tests en Red** — la eliminacion de tests obsoletos (si los hay, tras invertir las assertions) va en Refactor.

**Green (commit `fix:`)**
- En `ReportFormatter::format_report`, quitar la llamada a `format_consensus_summary()`.
- Marcar `format_consensus_summary` como `#[deprecated]` o eliminar si es privada.

**Refactor (commit `refactor:`)**
- Eliminar helper privada `format_consensus_summary` si ya no se invoca.
- Eliminar tests que quedaron redundantes tras la inversion en Red (si los hay).

---

### Seccion S07 — Dissent renderizado como una linea (summary-only) (G08)

**Archivo objetivo:** `src/reporting.rs` (`format_dissent`).

**Spec:**
```
## Dissenting Opinion

**Caspar (Critic)**: <summary unicamente>
```

**Red (commit `test:`)**
- `test_dissent_shows_one_line_per_dissenter`
- `test_dissent_line_contains_summary_not_reasoning`
- `test_dissent_section_has_blank_line_after`

**Green (commit `fix:`)**
- Reemplazar el cuerpo de `format_dissent`:
  ```rust
  fn format_dissent(&self, dissent: &[Dissent]) -> String {
      let mut out = String::new();
      writeln!(out, "\n## Dissenting Opinion\n").ok();
      for d in dissent {
          let (name, title) = self.agent_display(&d.agent);
          writeln!(out, "**{} ({})**: {}", name, title, d.summary).ok();
      }
      writeln!(out).ok();
      out
  }
  ```

**Refactor (commit `refactor:`)**
- Revisar tests de `format_report` que hacian assert sobre el reasoning en la seccion Dissent; eliminarlos o reenfocarlos. **La eliminacion va en Refactor, no en Red** — los tests obsoletos deben primero quedar en estado "passing pero redundante" antes de removerlos.

---

### Seccion S08 — Finding line sin parrafo de detail (G09)

**Archivo objetivo:** `src/reporting.rs` (`format_findings`).

**Spec:**
```
[!!!] **[CRITICAL]** <title> _(from Melchior, Caspar)_
```
Marker columna `_FINDING_MARKER_WIDTH = 5`, severity label columna `_FINDING_SEVERITY_WIDTH = 14`.

**Red (commit `test:`)**
- `test_findings_line_does_not_contain_detail_text`
- `test_findings_line_marker_column_is_5_chars_left_justified`
- `test_findings_line_severity_label_column_is_14_chars_left_justified`
- `test_findings_line_matches_python_layout_exactly`

**Green (commit `fix:`)**
- Agregar constantes:
  ```rust
  const FINDING_MARKER_WIDTH: usize = 5;
  const FINDING_SEVERITY_WIDTH: usize = 14;
  ```
- Reescribir `format_findings` para renderizar una unica linea por finding, con padding fijo, sin detail.
- `Severity::icon()` ya devuelve `[!!!]`, `[!!]`, `[i]` — verificar.

**Refactor (commit `refactor:`)**
- Documentar en docstring que `detail` sigue disponible via `ConsensusResult::findings[].detail` (JSON), solo no se renderiza en markdown.

---

### Seccion S09 — `majority_summary` con prefijo de agente (G10)

**Archivo objetivo:** `src/consensus.rs` (`determine`, paso 12).

**Spec:**
```
"Melchior: <summary> | Balthasar: <summary>"
```

**Red (commit `test:`)**
- Modificar `test_majority_summary_joins_with_pipe`:
  ```rust
  assert!(result.majority_summary.contains("Melchior: Melchior summary"));
  assert!(result.majority_summary.contains("Balthasar: Balthasar summary"));
  ```
- `test_majority_summary_uses_display_name_capitalized`

**Green (commit `fix:`)**
- Cambiar:
  ```rust
  let majority_summary = agents.iter()
      .filter(|a| a.effective_verdict() == majority_verdict)
      .map(|a| a.summary.as_str())
      .collect::<Vec<_>>()
      .join(" | ");
  ```
  por:
  ```rust
  let majority_summary = agents.iter()
      .filter(|a| a.effective_verdict() == majority_verdict)
      .map(|a| format!("{}: {}", a.agent.display_name(), a.summary))
      .collect::<Vec<_>>()
      .join(" | ");
  ```

**Refactor (commit `refactor:`)**
- Ninguno.

---

### Seccion S10 — Banner con alineacion de columnas y consensus con split (G11, dependencia de G06)

**Archivo objetivo:** `src/reporting.rs` (`format_banner`, nueva helper `fit_content`).

**Spec literal de `fit_content` (port 1:1 de Python `_fit_content`):**
```
fit_content(content: &str, width: usize, preserve_suffix: &str) -> String

Precondiciones (debug_assert + doc):
  - content y preserve_suffix son ASCII
    → fuerza con `debug_assert!(content.is_ascii() && preserve_suffix.is_ascii())`
    → en release, comportamiento UB si se viola: byte-slicing podria caer en medio
      de un codepoint multi-byte. Por eso la banner explicitamente documenta
      "ASCII contract" y agent_titles se valida en ReportConfig.
  - width > 0
    → debug_assert!(width > 0)

Invariante post: la longitud en bytes del resultado es:
  - len(content) si len(content) <= width (caso no-truncamiento)
  - exactamente width en otro caso

Algoritmo:
  1. Si content.len() <= width: return content.to_string()  // no truncar
  2. Constante ELLIPSIS = "..." (3 bytes)
  3. Fallback tail-cut (aplica si):
       preserve_suffix.is_empty() || preserve_suffix.len() + 3 >= width
     Resultado:
       cutoff = max(1, width - 3)
       content[..cutoff] + "..."
  4. Prefix-truncate con suffix protegido (caso comun):
       prefix_budget = width - 3 - preserve_suffix.len()
       assert prefix_budget >= 1  // garantizado por guard del paso 3
       prefix_source = &content[..content.len() - preserve_suffix.len()]
       prefix_source[..prefix_budget] + "..." + preserve_suffix
```

**Enforcement estatico de ASCII precondition (hardening):**
- `ReportConfig::new_checked(agent_titles: ...) -> Result<Self, ReportError>` valida ASCII de todos los strings al construir, retornando error estructurado.
- `ReportConfig::default()` es infallible porque hardcodea ASCII.
- Permite que `fit_content` asuma ASCII sin run-time panic.

**Decision ASCII enforcement — newtype vs discipline (resuelta):**
Se adopta la opcion **"discipline-based + runtime check at boundary"**, no newtype. Rationale:
- Newtype `AsciiString` requiere propagacion a traves de API publica → ruptura de ergonomia para el consumer.
- Runtime check en `ReportConfig::new_checked` es suficiente para atrapar violaciones en tests.
- `debug_assert!` en `fit_content` atrapa regressions en dev/test.
- En release: si `ReportConfig` fue construido con `default()` (infallible) o `new_checked` (validado), el invariante se mantiene; el byte-slicing es safe por construccion.
- **Documentar explicitamente** en rustdoc de `ReportConfig` que cualquier construccion fuera de estos dos metodos invalida el contrato.

Comportamiento bajo `--release` si la precondicion se viola (por ejemplo, consumer construye `ReportConfig` via struct literal con non-ASCII): `fit_content` retorna un String potencialmente corrupto o **panic** si el byte-slice cae en medio de un codepoint UTF-8 (no UB en Rust — slice de `&str` con boundary inválido hace panic, no memoria no inicializada). Se acepta este failure mode como "loud failure" vs UB silencioso.

**Spec de `format_banner`:**
1. Computar `labels = agents.iter().map(agent_label).collect::<Vec<_>>()` donde `agent_label(a) = format!("{} ({}):", display_name, title)`.
2. `max_label_len = labels.iter().map(|l| l.len()).max().unwrap_or(0)`.
3. Por cada agente: construir `verdict_suffix = format!(" {} ({}%)", VERDICT_UPPER, pct_int)`.
4. `content = format!("  {label:<max_label_len}{verdict_suffix}")`.
5. `fitted = fit_content(&content, banner_inner, &verdict_suffix)`.
6. Line = `format!("|{:<width$}|", fitted, width = banner_inner)`.
7. Linea CONSENSUS: `content = format!("  CONSENSUS: {}", consensus.consensus)`; `fitted = fit_content(&content, banner_inner, "")` (sin suffix protegido).

**Red (commit `test:`)**
- `test_banner_labels_are_column_aligned_to_max_label_len`
- `test_banner_verdict_preserved_when_label_exceeds_width`
- `test_banner_consensus_line_includes_split_for_go_with_caveats` (integra con S05)
- `test_banner_all_lines_are_exactly_banner_width` (ya existe, mantener)
- `test_fit_content_returns_input_when_shorter_than_width`
- `test_fit_content_returns_input_when_exactly_width`
- `test_fit_content_preserves_suffix_when_prefix_overflows`
- `test_fit_content_falls_back_to_tail_cut_when_no_suffix`
- `test_fit_content_falls_back_to_tail_cut_when_suffix_plus_ellipsis_exceeds_width`
- `test_fit_content_ellipsis_is_exactly_three_dots`
- `test_fit_content_resulting_length_equals_width_when_truncated`
- `test_fit_content_boundary_width_1` (fallback: "x" + cut si width=1, edge case del assert)
- `test_fit_content_matches_python_vectors` — fixture `tests/fixtures/fit_content_vectors.jsonl` con tuplas `{content, width, suffix, expected}` generadas con Python `_fit_content`.

**Green (commit `feat:`)**
- Implementar helper privada con firma exacta:
  ```rust
  fn fit_content(content: &str, width: usize, preserve_suffix: &str) -> String
  ```
  reproduciendo los 4 pasos de la spec literal arriba.
- Reescribir `format_banner` para usar `max_label_len` y `fit_content`.

**Refactor (commit `refactor:`, opcional)**
- Extraer constantes `BANNER_WIDTH`, `BANNER_INNER` como `const` publicos en el modulo si el consumidor los necesita.

---

### Seccion S11 — **MOVIDA a v0.3.0**

**Decision (ratificada tras R3 MAGI):** la consolidacion de prompts 9 → 3 archivos + hardening anti-inyeccion esta fuera del scope de `v0.2.0`. Ver plan dedicado:

**`planning/claude-plan-tdd-v0.3-prompts.md`**

Razones del split:
- S11 es arquitectural y acumula mas complejidad que el resto del plan combinado.
- Ship de v0.2.0 sin S11 ya cierra 10 de 11 gaps criticos del gap analysis.
- v0.3.0 dedicado permite MAGI review focalizado + ADR de threat model + ventana rc.1 formal.

El gap G02 del gap analysis queda pendiente para v0.3.0. Los gaps restantes (G01, G03–G13, G15) estan cubiertos por las secciones S01–S10, S12, S13 de este plan.

---

### Seccion S12 — Validator reemplaza title in-place (G13)

**Archivo objetivo:** `src/validate.rs`.

**Decision de API:**
- Opcion A: `fn validate_mut(&self, output: &mut AgentOutput) -> Result<(), MagiError>` que limpia in-place.
- Opcion B: `fn validate(&self, output: AgentOutput) -> Result<AgentOutput, MagiError>` que consume y devuelve.
- **Preferida: A** (evita copias, mas cercano al patron Python).

**Red (commit `test:`)**
- `test_validate_mut_replaces_title_with_cleaned_form`
- `test_validate_mut_strips_zero_width_from_titles`
- `test_validate_mut_collapses_control_whitespace_in_titles`
- `test_validate_mut_preserves_order_of_findings`
- `test_validate_retains_original_behavior_on_immutable_slice` (el metodo inmutable sigue existiendo)

**Green (commit `feat:`)**
- Agregar `pub fn validate_mut(&self, output: &mut AgentOutput) -> Result<(), MagiError>`.
- Antes de validar longitud de title, asignar `f.title = clean_title(&f.title)`.
- Mantener `validate(&self, &AgentOutput)` inmutable como API existente.

**Refactor (commit `refactor:`)**
- En `orchestrator.rs`, usar `validate_mut` en lugar de `validate` en el pipeline de parseo.

---

### Seccion S13 — Default de `max_input_len` a 10 MB (G12)

**Archivo objetivo:** `src/orchestrator.rs` (`MagiConfig::default`).

**Analisis de memory envelope (reanalizado tras feedback de MAGI R2):**

El analisis previo decia "content × 2", pero subestimaba la realidad del pipeline actual. Reanalisis con el codigo real de `orchestrator.rs`:

1. **Input user:** `&str` recibido por `analyze()` → **0 allocs** (referencia).
2. **Sanitizacion S11 (Capa 1):** reemplazos via regex replace → **1 alloc de ~content_len** (string sanitizado).
3. **Construccion del user_prompt con delimitadores:** `format!("MODE: ...\n---BEGIN...\n{content}\n---END...\n")` → **1 alloc de ~content_len + 120 bytes**.
4. **Clonacion a cada agente:** si se usa `String::clone` por agente → **N × content_len** (3 copias para 3 agentes). Si se usa `Arc<str>` → **0 allocs adicionales** (solo clonacion del pointer).
5. **Serializacion al wire:**
   - `ClaudeProvider` (HTTP): `serde_json::to_string(&request)` crea JSON body → **1 alloc × content_len × 3** (una por request, con overhead de escape JSON ~1.1x).
   - `ClaudeCliProvider` (subprocess): escritura a stdin via `tokio::io::AsyncWriteExt::write_all` — buffered internamente por tokio, probablemente **1 alloc × content_len × 3**.

**Pico realista para content_len = 10 MB:**
- Sin `Arc<str>`: **~50 MB** (1 sanitizado + 1 prompt + 3 clonados + 3 wire) — no 20 MB.
- Con `Arc<str>` en el pipeline orchestrator→agent: **~40 MB** (1 sanitizado + 1 prompt + 3 wire).

**Accion prescriptiva:**
- **Verificar antes de S13:** leer `orchestrator.rs::analyze()` y contar los puntos reales de allocation. Si se encuentran copias innecesarias, documentarlas aqui y no subir el default sin arreglarlas.
- **Opcion conservadora:** bajar el default propuesto de 10 MB a **4 MB** si el pipeline hace copias por agente. 4 MB × ~5 copias = 20 MB, aceptable.
- **Opcion alineada con Python:** 10 MB si el pipeline ya usa `Arc<str>` o se refactoriza para usarlo (sub-seccion opcional de S13).

**Decision inicial:** default = `4 * 1024 * 1024` (4 MB) — mas conservador que Python (10 MB) pero 4x mas permisivo que v0.1.2 (1 MB). Subir a 10 MB en v0.3.0 tras auditar allocs.

**Hard precondition de S13:** antes del commit Red de esta seccion, ejecutar el audit de allocs contra `src/orchestrator.rs::analyze()` actual e **inlinear los resultados** en este documento — no es "advisory". Especificamente:
1. Grep de `.to_string()`, `String::from`, `format!`, `.clone()` en el pipeline `analyze → Agent::execute → Provider::complete`.
2. Contar copias reales de `content` entre entry-point y wire-serialization.
3. Si el conteo excede 5 copias, o si se detecta una copia innecesaria, **crear un issue y resolverlo antes de subir el default**.
4. Documentar el resultado del audit en este archivo (seccion S13, sub-seccion "Audit Results (YYYY-MM-DD)").

**Audit Results (2026-04-18):**

Pipeline auditado: `analyze()` → `launch_agents()` → `Agent::execute()` → `LlmProvider::complete()`.

Copy points de `content` (embebido en `user_prompt` = result de `build_prompt()`):

| # | Ubicacion | Tipo | Nota |
|---|-----------|------|------|
| 1 | `orchestrator.rs:289` — `build_prompt()` | `format!` | Crea `String` de ~content_len + 20 bytes |
| 2–4 | `orchestrator.rs:338` — `prompt.to_string()` (x3, dentro del loop `for agent in agents`) | `.to_string()` | Necesario: `tokio::spawn` requiere ownership `'static`; una copia por agente |
| 5 | `providers/claude.rs:147` — `user_prompt.to_string()` en `build_request_body` | `.to_string()` | Solo en el path HTTP; la copia queda en `ClaudeRequest.messages[0].content` |
| 6 | `providers/claude.rs:214` — `.json(&body)` | serde+reqwest | Serializa `ClaudeRequest` a JSON bytes para el wire; no es una `String` allocation sino `Bytes` |
| CLI | `providers/claude_cli.rs:195` — `stdin.write_all(user_prompt.as_bytes())` | write-only | Sin allocation extra; escribe bytes directamente desde `&str` |

**Conteo de copias de `content` al wire:**
- `ClaudeProvider` (HTTP): 1 (build_prompt) + 3 (per-agent clone) + 1 (build_request_body) = **5 copias**. Dentro del limite.
- `ClaudeCliProvider` (subprocess): 1 (build_prompt) + 3 (per-agent clone) = **4 copias**. Dentro del limite.

**Copias innecesarias detectadas:** ninguna. Las 3 clones por agente son obligatorias para satisfacer `'static` en `tokio::spawn`. El `.to_string()` en `build_request_body` es necesario porque `ClaudeRequest` toma ownership (requerido por `#[derive(Serialize)]` y movido a `reqwest::json()`).

**Veredicto:** PROCEED — maximo 5 copias, no se detectan copias evitables en el pipeline actual. El default de 4 MB implica pico de ~20 MB (5 × 4 MB), que es aceptable. Refactorizar a `Arc<str>` para reducir a 4 copias es una mejora opcional deferida a v0.3.0.

**Distincion raw-vs-sanitizado (rustdoc):** el limite `max_input_len` aplica al **input raw** antes de sanitizacion (para evitar que un content con muchos zero-width chars pase el chequeo y luego infle la memoria durante el strip). Documentar:
```rust
/// Maximum accepted size of the raw `content` argument to `analyze()`, in bytes.
///
/// Measured BEFORE sanitization — a content with heavy zero-width padding is
/// rejected by this limit even if its sanitized form would fit. This choice
/// prevents sanitization-time allocation blowup from adversarial inputs.
pub max_input_len: usize,
```

**Mitigacion:** el default es solo un default. Consumidores con exposicion publica deben usar `MagiBuilder::with_max_input_len(1_048_576)` explicitamente. Documentar esta responsabilidad en rustdoc.

**Red (commit `test:`)**
- `test_magi_config_default_max_input_len_is_4mb`:
  ```rust
  assert_eq!(MagiConfig::default().max_input_len, 4 * 1024 * 1024);
  ```
  Este test **falla en v0.1.2** porque el default actual es 1 MB — es Red genuino.
- `test_builder_with_max_input_len_overrides_default` — test nuevo, debe fallar a nivel de compilacion si `with_max_input_len` no existe aun, o fallar a nivel de assertion si ya existe con otro comportamiento. Verificar antes de commitear Red que falla.
- **Si existe** un test del tipo `test_analyze_rejects_content_exceeding_1mb`, renombrarlo/modificar su assertion a `test_analyze_rejects_content_exceeding_4mb` con el valor nuevo — el cambio de literal lo pone en Red inmediatamente.
- **Precondicion de TDD-Guard:** ejecutar `python run-tests.py` tras el commit de Red y **verificar que al menos un test falla**. Si todos pasan, el Red no es genuino y debe ajustarse antes de proceder a Green.

**Green (commit `fix:`)**
- Cambiar literal `1_048_576` → `4 * 1024 * 1024` en `MagiConfig::default`.

**Refactor (commit `refactor:`, opcional)**
- Exportar `pub const DEFAULT_MAX_INPUT_LEN: usize = 4 * 1024 * 1024;` desde `orchestrator.rs` para que consumidores puedan referenciarlo.
- Documentar en rustdoc de `MagiConfig`: "Note: for public-facing deployments where `content` is untrusted, consider lowering this to a value appropriate for your threat model. Default (4 MB) is a compromise between Python's 10 MB and v0.1.2's 1 MB; a full 10 MB alignment with Python is deferred to v0.3.0 pending allocation audit of the analyze() pipeline."

---

### Seccion S14 — Parser robusto de salida Claude (G14) **[DIFERIDO a v0.2.1]**

**Decision:** **fuera de scope de v0.2.0**. El parser actual de `ClaudeCliProvider` ya funciona con el shape que emite `claude -p --output-format json` en la version actual del CLI; los shapes adicionales son defensivos contra cambios futuros del CLI o uso desde HTTP API sin `--output-format json`. Mantener el plan como referencia para **v0.2.1** (patch release post-v0.2.0 estable).

**Archivo objetivo (cuando se retome):** `src/providers/claude_cli.rs` y potencialmente `src/providers/claude.rs`.

**Spec (referencia):** aceptar tres shapes post-decodificacion JSON:
1. `{"result": "..."}` (envelope CLI actual)
2. `{"content": [{"type": "text", "text": "..."}, ...]}` (Anthropic API native)
3. string plano

Tras extraer el texto, aplicar `strip_code_fences` (ya existe).

**Tests diferidos:**
- `test_extract_text_from_result_envelope`
- `test_extract_text_from_content_text_blocks`
- `test_extract_text_from_plain_string`
- `test_extract_text_rejects_unknown_shape`

**Implementacion diferida:**
- `fn extract_text(raw: &serde_json::Value) -> Result<String, ProviderError>` con los tres branches.
- Ubicar en modulo compartido si `ClaudeProvider` HTTP lo necesita tambien.

---

## 3. Orden de ejecucion recomendado

Las secciones son mayormente independientes, pero hay dependencias menores:

```
S01 (opus alias)                    — independiente
S02 (clean_title)                   — base para S03, S12
  └── S03 (NFKC + casefold real)    — depende de S02
S04 (conditions from summary)       — independiente
S05 (GO WITH CAVEATS split)         — base para S10
  └── S10 (banner + fit_content)    — depende de S05
S06 (remove Consensus Summary)      — independiente
S07 (dissent one-line)              — independiente
S08 (findings line no detail)       — independiente
S09 (majority_summary prefix)       — independiente
S11 (prompts)                       — MOVIDA a v0.3.0 (plan separado)
S12 (validate_mut)                  — depende de S02
S13 (4 MB default)                  — independiente
S14 (extract_text)                  — DIFERIDO a v0.2.1
```

**Scope v0.2.0 — "algorithmic + report equivalence" (12 secciones, sin S11):**
1. **Bloque 1 — calentamiento:** S01, S13.
2. **Bloque 2 — Unicode (critico):** S02 → S03 → S12.
3. **Bloque 3 — reporte/consensus (logica visible):** S04, S05, S06, S07, S08, S09.
4. **Bloque 4 — banner:** S10 (requiere S05 verde).

**v0.3.0 — "prompt architecture equivalence":** el trabajo de prompts (ex-S11) se ejecuta como proyecto separado tras v0.2.0 estable. Ver `planning/claude-plan-tdd-v0.3-prompts.md`.

**Compatibilidad con TDD-Guard (resolucion del conflicto per-seccion):**
TDD-Guard enforcea fase-por-fase (Red → Green → Refactor en commits separados). **El plan NO contradice esto** — cada seccion sigue teniendo commits separados test:/feat:|fix:/refactor:, TDD-Guard los intercepta uno a uno. "Commits agrupados por seccion" se refiere a **aprobacion del usuario** (una presentation al usuario cubre los 2–3 commits de una seccion), no a commits fusionados.

**Cadencia de commits:** aprobacion **por seccion**, no por fase. Cada seccion genera 2 o 3 commits secuenciales (test → feat/fix → refactor opcional) que se presentan juntos al usuario. Esto reduce de 42–75 round-trips a ~13 (uno por seccion, incluso menos si Bloque 1 y Bloque 2 se presentan juntos).

**Totales reales (sin S11):**
- Commits: ~40–55 (12 secciones × 2–3 commits por seccion, menos los del S11 removido).
- Tests nuevos: ~65 (contados sumando las listas Red de S01–S10, S12, S13).

---

## 4. Verificacion y cierre

**Fase de pre-release (v0.2.0-rc.1):**
1. Al completar Bloques 1–4 (todas las secciones excepto S11 diferida), ejecutar **MAGI Round de v0.2.0** (3 agentes Opus) sobre el diff completo v0.1.2 → HEAD.
2. Si Round es STRONG GO o GO WITH CAVEATS sin criticos, proceder con rc.
3. Publicar `v0.2.0-rc.1` a crates.io con tag `-rc.1`.
4. Ventana de feedback interno: 48–72h (o omitir si no hay consumer nombrado).

**Fase de release (v0.2.0 final):**
5. Incorporar feedback de la rc.
6. **Tests finales** objetivo: ≥ 235 tests (172 actuales + ~65 nuevos).
7. **Coverage de rustdoc** ≥ 95%.
8. **CHANGELOG.md** con todas las notas de breaking changes.
9. **Migration guide** en `docs/migration-v0.2.md` para consumidores:
    - `stripped_title()` deprecado → usar `clean_title()` (shim mantenido en v0.2.x, removido en v0.3)
    - Seccion `## Consensus Summary` eliminada del reporte markdown (consumidores parsean `consensus.majority_summary` del JSON)
    - `Condition.condition` ahora viene de `summary` no `recommendation`
    - Alias `opus` resuelve a `claude-opus-4-7`
    - Default de `max_input_len` subio de 1 MB a 4 MB — consumidores con exposicion untrusted deben setearlo explicitamente
    - `ConsensusEngine::dedup` ya no colapsa whitespace interior — titulos con multi-space ahora son distintos (alineado con Python)
    - Nueva dependencia: `unicode-normalization` y `caseless` (pin tilde)
    - **Nota forward-looking:** el cambio de arquitectura de prompts (9 archivos → 3 archivos + hardening anti-inyeccion) se entrega en `v0.3.0` — los consumidores que usan `with_custom_prompt(agent, mode, prompt)` no se ven afectados en v0.2.0.
10. **Tag** `v0.2.0` y publicar a crates.io.

---

## 5. Checklist de verificacion por seccion

**Despues de cada bloque (no por fase individual):**
- [ ] `python run-tests.py` — 0 failures
- [ ] `cargo clippy --tests -- -D warnings` — 0 warnings
- [ ] `cargo fmt --check` — clean
- [ ] `cargo build --release` — sin warnings
- [ ] `cargo doc --no-deps` — sin doc warnings
- [ ] `cargo audit` — sin vulnerabilidades conocidas
- [ ] `cargo outdated --depth 1` — reporte informativo de deps desactualizadas
- [ ] `cargo tree | grep -E "(unicode-normalization|caseless)"` — confirmar deps nuevas presentes con pins tilde

**Commits:**
- Prefijo obligatorio: `test:` / `feat:` / `fix:` / `refactor:`
- Mensaje en ingles
- Sin `Co-Authored-By`, sin mencion a Claude/AI
- Nunca commitear sin instruccion explicita del usuario
- **Refactor commits vacios se omiten** — si no hay trabajo real de limpieza tras Green, se salta.

## 6. Changelog de correcciones MAGI Round 1 (2026-04-18)

Este plan fue revisado por MAGI Round 1 (consenso `GO WITH CAVEATS (3-0)`) y actualizado para resolver todos los hallazgos criticos y warnings:

**Criticos resueltos:**
- ✅ S03 casefold: adoptado crate `caseless`, no `to_lowercase`. Sin tests `#[ignore]`.
- ✅ S11 prompt injection: tests adversariales agregados, sanitizacion via delimitadores `---BEGIN/END USER CONTEXT---` + neutralizacion de lineas `MODE:` en content.

**Warnings resueltos:**
- ✅ S02 zero-width set mirror literal de Python con coverage per-codepoint (17 tests).
- ✅ S10 `fit_content` spec literal 1:1 con pseudo-codigo + 11 tests incluyendo corpus Python.
- ✅ S11 retencion de `(AgentName, Option<Mode>)` como key y shim 3-args deprecado.
- ✅ Contrato de formato numerico agregado al preambulo (confidence, porcentajes, split).
- ✅ S02+S03 divergencia de interior whitespace: documentada + test explicito; S03 ya no normaliza whitespace (matchea Python).
- ✅ S11 builder API: shim con `#[deprecated]` en lugar de breaking directo.
- ✅ S05 edge tests degraded + boundaries de epsilon.
- ✅ Deps pin estricto: `unicode-normalization = "=0.1.24"`, `caseless = "=0.2.2"`; `regex` ya es dep directa (no se agrega).
- ✅ S13 memory envelope documentado.
- ✅ Soft-landing con v0.2.0-rc.1 + 7 dias feedback.
- ✅ S14 diferido a v0.2.1 (decidido).
- ✅ Effort estimate corregido: ~60–75 commits, ~95 tests, 3–5 dias.

**Info resueltos:**
- ✅ Refactor commits vacios se omiten.
- ✅ Test target actualizado de 50 a ~95.
- ✅ Byte-equivalence justificado en preambulo.
- ✅ Commit cadence por seccion (reduce round-trips).
- ✅ LazyLock MSRV check: estable desde Rust 1.80; MSRV 1.91 ok.

## 7. Changelog de correcciones MAGI Round 2 (2026-04-18)

MAGI Round 2 subio a `GO WITH CAVEATS (3-0)` con Melchior en APPROVE(88%). 3 criticos y 12 warnings legitimos resueltos:

**Criticos Round 2 resueltos:**
- ✅ **Arity conflict Rust:** `with_custom_prompt` 2-arg renombrado a `with_custom_prompt_all_modes`; 3-arg retenida como deprecated canonical. Sin colision de firmas en el mismo `impl`.
- ✅ **Injection mitigation insuficiente:** reescrita con 4 capas (strip zero-width, normalizar CRLF, neutralizar 4 prefijos incluyendo `---BEGIN/END`, nonce hex-16 por peticion, fail-closed si colisiona). ADR `docs/adr/001-prompt-injection-threat-model.md` mandatorio antes de S11.
- ✅ **Dedup ordering no contractual:** contrato explicito `first_seen_by_agent_iteration_order` + 4 tests de regression; data structure cambia de `HashMap` a `Vec<(String, GroupState)>` con linear scan.

**Warnings Round 2 resueltos:**
- ✅ S05 test name corregido (eliminado el `go_with_caveats_*` que contradecia expected outcome).
- ✅ S13 memory envelope reanalizado: pico real ~50 MB con 10 MB input, default bajado a **4 MB** (compromiso), 10 MB deferido a v0.3.0 pendiente de audit.
- ✅ Fixture generation: script `tests/fixtures/generate.py` especificado con MAGI_REF_SHA pin + politica de regeneracion + cabecera de auditoria.
- ✅ Dep pins: cambiados de `=` estricto a `~` tilde (`~0.1.24`, `~0.2.2`) — balance entre reproducibilidad y semver-patch compat.
- ✅ Caseless stale: documentado como aceptable (funcion pura, sin unsafe, sin deps); alternativa `icu_casemap` citada.
- ✅ Rollback strategy explicita: revert del range del bloque, yank si ya hay rc publicada, sin feature gates.
- ✅ rc.1 window reducida a **48–72h** (o omitir si no hay consumer nombrado).
- ✅ Decision gate explicito al cerrar Bloque 3 para S11 → v0.2.0 vs v0.3.0.
- ✅ S10 `fit_content` ASCII precondition: `debug_assert` + `ReportConfig::new_checked` para enforcement estatico.
- ✅ S11 injection regex ampliada: cubre `MODE|CONTEXT|---BEGIN|---END` en vez de solo `MODE`.
- ✅ S11 backcompat test simplificado: solo 3 archivos en v0.2.0; per-mode directory backcompat deferido a v0.3.0.
- ✅ `rand` agregado como dep nueva para nonce generation.

**Info resueltos:**
- ✅ Greek sigma test clarificado como casefolding puro, no compatibility folding.
- ✅ S05 degraded expectation re-derivada explicitamente en la descripcion del test.
- ✅ Nota de reversibilidad integrada en rollback strategy.
- ✅ Per-section commit cadence reafirmada.

## 8. Changelog de correcciones MAGI Round 3 (2026-04-18)

MAGI Round 3 reporto `GO WITH CAVEATS (3-0)` con 1 critico y 15 warnings. Melchior bajo a CONDITIONAL(82%) pero principalmente por el bug del nonce formatter. Correcciones aplicadas:

**Critico Round 3 resuelto:**
- ✅ **Nonce formatter bug:** `{:x}` → `{:032x}` (zero-padded 32 hex chars). Sin padding, bits altos en cero producen strings cortas y tests estocasticos. Test ahora usa fixtures deterministicos (u128 = 0x3 y u128::MAX) en lugar de nonces aleatorios.

**Warnings Round 3 resueltos:**
- ✅ **S11 split a v0.3.0 por default:** aceptada la recomendacion de Balthasar. v0.2.0 = "algorithmic + report equivalence" (S01–S10, S12, S13). v0.3.0 = "prompt architecture equivalence" (S11 con ADR + MAGI review dedicado).
- ✅ **Zero-width name rename:** `ZERO_WIDTH_RE` → `INVISIBLE_AND_SEPARATOR_RE` con documentacion de que incluye separadores visibles (U+2028, U+2029, U+202F) y bidi marks, no solo zero-width reales.
- ✅ **S13 audit como hard precondition:** audit de allocs pre-Red mandatorio, resultados inlined en el documento. Sin ambiguedad "advisory".
- ✅ **caseless vendored fallback:** plan detallado si upstream es comprometido/retirado (vendor + path dep + CHANGELOG). Alternativas `icu_casemap` y tabla custom citadas.
- ✅ **Injection scope explicitado:** rustdoc de `analyze()` lista que SI defiende (literal injection, hiding via invisibles) y que NO defiende (semantica, jailbreaks, side-channel, exfil via output).
- ✅ **S10 ASCII decision resuelta:** discipline-based + runtime check en `new_checked` + debug_assert, NO newtype. Fallure mode bajo release: panic limpio (no UB) si se viola.
- ✅ **Fixture integrity:** `.sha256` hermano commiteado + CI check.
- ✅ **Python dep friction aceptada en v0.2.0:** Python ya esta requerido. v0.3.0 considera port a Rust con pyo3.
- ✅ **S13 raw-vs-sanitizado clarificado:** rustdoc especifica que `max_input_len` aplica al input RAW (pre-sanitizacion).
- ✅ **Dedup O(m²) documentado** con comentario obligatorio en codigo + limite de re-evaluacion.
- ✅ **Greek ς casefold ambiguity:** test ejecuta comparacion Python/caseless antes de fijar expected; testear solo `Σ == σ` si `ς` divergge.
- ✅ **Idempotencia de `clean_title`:** test agregado.
- ✅ **TDD-Guard compatible:** clarificacion — "agrupar por seccion" = aprobacion del usuario, no merge de commits. TDD-Guard sigue interceptando fase-por-fase.
- ✅ **rand → fastrand:** justificado como mejor para nonce no-cripto (mas ligero, menos deps transitivas).
- ✅ **Rollback rehearsal:** dry-run obligatorio en branch de prueba antes del primer merge a main.

**Info resueltos:**
- ✅ `fit_content` comportamiento bajo release documentado (panic limpio, no UB).
- ✅ `clean_title` idempotence test.
- ✅ Migration guide priorizado como deliverable clave.
- ✅ rc.1 window condicional mantenida.
- ✅ `fastrand` vs `rand` trade-off citado.
- ✅ Rollback revert-ability rehearsada.

---

**Nota sobre rendimientos decrecientes:** este plan ha sido revisado en 3 rondas MAGI consecutivas. Cada ronda el consenso es `GO WITH CAVEATS (3-0)`, con scores oscilando en 72–88%. Los "conditional" en R3 son mas cosmeticos que correctivos (preferencias de scope, opinion sobre orden de delivery). El plan esta **listo para ejecucion**. Iterar a R4 tendria retornos marginales y potencialmente agregaria scope creep.
