# BMAD Dev Cycle Agents × Non-Frontier Models: Role Assignment Guide

## Executive Summary

For the BMAD development cycle (SM → PO → DEV → QA), three non-frontier open-weight models — **GLM-5.2**, **Kimi K2.7-Code**, and **DeepSeek V4 Pro** — each have meaningfully different capability profiles that map cleanly to specific agent roles. GLM-5.2 leads on strategic planning, instruction adherence, and broad agentic intelligence. Kimi K2.7-Code leads on efficient, multi-turn, tool-rich code execution. DeepSeek V4 Pro sits in the middle — very strong on algorithmic and pure-logic reasoning but slightly weaker on holistic planning quality. **Party Mode** (the collaborative brainstorming/hashing-out tool) benefits from a different model selection logic entirely, where breadth of perspective and conversational nuance matter more than raw code throughput.

***

## BMAD Agent Roles: What Each One Actually Needs

The four dev-cycle agents are defined in BMAD's agent reference as follows:[^1]

| Agent | Persona | Primary Job | Key Cognitive Demands |
|-------|---------|-------------|----------------------|
| **SM (Bob)** | Technical Scrum Master | Drafts crystal-clear stories from PRD/arch docs, plans sprints, handles course correction | Structured synthesis; disciplined scope control; long-context reading; precise instructions |
| **PO (Sarah)** | Technical Product Owner & Process Steward | Validates story drafts against PRD, shards docs, guards artifact cohesion | Nuanced judgment; requirement tracing; gatekeeping; consistency across long docs |
| **DEV (James/Amelia)** | Expert Senior Software Engineer | Multi-file story implementation, test writing, code review, pattern adherence | Agentic tool orchestration; long-horizon code gen; multi-turn memory; speed |
| **QA (Quinn)** | Test Architect w/ Quality Advisory Authority | Risk-based review, test scenario design, Given-When-Then tracing, quality gates | Analytical depth; requirements traceability; adversarial thinking |

The SM and PO roles are about *judgment and structured synthesis* — they read specs and produce clean artifacts for other agents. The DEV role is about *agentic code execution at scale* — producing correct, tested code across many files. The QA role demands *analytical rigor and adversarial reasoning* about what could go wrong.[^2][^1]

***

## The Three Models: Capability Profiles

### GLM-5.2 (Z.ai)

Released June 13, 2026, GLM-5.2 is a 744B-parameter MoE (40B active) model with a usable 1M-token context window, MIT license, and two reasoning effort levels (high / max). It leads all open-weight models on the Artificial Analysis Intelligence Index v4.1 with a score of **51**, ahead of DeepSeek V4 Pro (44) and competitive with Claude Opus 4.8 (56) and GPT-5.5 xhigh (55).[^3][^4][^5]

**Coding benchmarks:**
- SWE-bench Pro: **62.1%** (beats GPT-5.5 at 58.6%; near Claude Opus 4.8)[^4][^5]
- FrontierSWE: **74.4%** (within 1 point of Claude Opus 4.8 at 75.1%)[^4]
- Terminal-Bench 2.1: **81.0** (up from GLM-5.1's 63.5, second highest open-weight score)[^3][^4]
- MCP-Atlas (tool use): **77.0** (just below Opus 4.8's 77.8)[^4]
- GDPval-AA (agentic knowledge work): **1524 Elo** — highest open-weight score, above GPT-5.5 (1514)[^5]

**Planning/reasoning benchmark:**
- AIME 2026: **99.2** (math/logic)[^4]
- GPQA-Diamond: **91.2** (graduate-level science)[^4]
- HLE (with tools): **54.7** (near Opus 4.8's 57.9)[^4]

**Key characteristics for agentic use:** 1M context holds stably across long agent runs; high output quality on structured document synthesis; Anthropic-compatible API integrates directly into Claude Code and Cline with a base URL swap. Known limitation: burns ~43K output tokens per AA-Briefcase task (vs ~24K for MiniMax-M3) — token-hungry in complex sessions. No native vision support at launch. Also known to occasionally misidentify itself as Claude.[^5][^3]

In blind head-to-head evaluations on real decisions (policy compliance, multi-team resource allocation, trend analysis with conflicting data), GLM-5.2 scored **80.7 average vs. DeepSeek V4 Pro's 69.7** — a significant gap on judgment-heavy, strategic tasks.[^6]

***

### Kimi K2.7-Code (Moonshot AI)

Released June 12, 2026, Kimi K2.7-Code is a 1-trillion-parameter MoE (32B active) model with 262K context (256K usable), built specifically for agentic coding tasks and MCP tool orchestration. It is multimodal (text, image, video via MoonViT), always runs in forced thinking mode, and preserves reasoning content across multi-turn interactions.[^7][^8]

**Coding benchmarks:**
- Kimi Code Bench v2: **62.0** (+21.8% over K2.6)[^9][^7]
- Program Bench: **53.6** (+11.0% over K2.6)[^7]
- MLS Bench Lite: **35.1** (+31.5% over K2.6)[^7]
- MCP Mark Verified: **81.1%** (+8.3 over K2.6) — tool call accuracy across real MCP environments[^10][^8]
- MCP-Atlas: **76.0%** (+6.6 over K2.6)[^8]
- Terminal-Bench Hard: **44.7** (solid but below GLM-5.2 and DeepSeek V4 Pro)[^11]

**Efficiency characteristic:** ~30% fewer thinking tokens than K2.6 on equivalent agentic tasks — efficiency without sacrificing completion quality. MCP tool orchestration across Notion, GitHub, Filesystem, Postgres, and Playwright is a first-class design goal.[^12][^8]

**Known strength:** Optimized for real-world long-horizon coding tasks — production incidents, open-source contributions, backend services, infrastructure, ML/data work, security engineering, multi-file refactors. The forced thinking + preserve_thinking design means reasoning context accumulates and carries over across a full coding session, which is exactly what the BMAD DEV agent loop needs.[^8][^7]

**Key limitation:** 256K context (vs GLM-5.2's 1M) and self-reported benchmarks dominate the numbers — independent third-party validation is still sparse. Artificial Analysis Intelligence Index score: 41.9, notably below GLM-5.2's 51.[^13][^11]

***

### DeepSeek V4 Pro (DeepSeek AI)

Released April 24, 2026, DeepSeek V4 Pro is a 1.6T-parameter MoE (49B active) model with a native 1M context window, MIT license, and three reasoning modes (non-thinking, thinking, max-thinking). It ships at $1.74/$3.48 per 1M tokens — the most cost-efficient of the three.[^14][^15][^16]

**Coding benchmarks:**
- SWE-bench Verified: **80.6%** — very strong absolute number but a now-saturated benchmark[^17][^14]
- SWE-bench Pro: **55.4%** — below GLM-5.2 (62.1%) and behind Kimi K2.7 on agentic tasks[^18][^4]
- LiveCodeBench (contamination-resistant): **93.5** — highest among the three models here[^19][^20]
- Codeforces CodeElo: **3206 Elo** (~rank 23 among human contestants) — strongest algorithmic/competitive coding of the group[^19]
- MCP-Atlas: **73.6%** — below both GLM-5.2 and Kimi K2.7[^19][^4]
- Terminal-Bench 2.0: **67.9** — trails GLM-5.2 on long-horizon agentic shell work[^19]

**Reasoning/planning characteristic:** DeepSeek V4 Pro explicitly supports tool calls *inside* reasoning steps — it can plan, call tools, integrate results, and iterate in a single call. However, in head-to-head judgment tests (strategic planning, policy compliance, resource allocation), it scored notably below GLM-5.2. Community consensus suggests DeepSeek V4 Pro is a "highly competent model and great value" but GLM-5.2 has a significant advantage "for intricate tasks such as strategy development, architecture, and planning sessions".[^21][^6]

**Key advantage:** Price. At ~$0.87/M output tokens vs GLM-5.2's $4.40/M, V4 Pro is the cost-optimized workhorse for high-volume, well-defined coding tasks. Also strongest on pure algorithmic reasoning (LiveCodeBench #1 in this group).[^15][^18]

***

## Model-to-Agent Role Assignments

### SM (Scrum Master) → **GLM-5.2**

**Reasoning:** The SM's job is to read the PRD and architecture document, synthesize a coherent sprint plan, and produce crystal-clear story files that leave no ambiguity for the DEV agent. This is a structured document synthesis task operating over large, interconnected specs. It requires:[^1]

- Long-context comprehension across the full PRD + architecture (~50-100K tokens minimum for complex projects)
- Disciplined instruction-following to produce story files that strictly adhere to templates
- Holistic judgment to detect scope gaps, missing acceptance criteria, or story-architecture misalignments

GLM-5.2's **1M context window** means it can load the entire PRD, architecture doc, and existing story backlog simultaneously. Its **GDPval-AA score of 1524** (highest open-weight) confirms its quality on knowledge-work synthesis tasks that mirror exactly what SM does. Its **GPQA-Diamond 91.2** and **HLE-with-tools 54.7** scores reflect the reasoning quality needed to evaluate story dependencies and catch edge cases in requirements.[^3][^5][^4]

The SM never writes code — it produces structured natural language artifacts. GLM-5.2's advantage is in that exact domain: synthesizing complex input into well-structured output under tight constraints. DeepSeek V4 Pro's lower planning scores (69.7 vs GLM-5.2's 80.7 in blind judgment tests) would show up here as shallower story quality.[^6]

***

### PO (Product Owner) → **GLM-5.2**

**Reasoning:** PO is the "process steward" and "guardian of quality and completeness". Its primary command `*validate-story-draft {story}` requires reading a story draft and the source PRD/architecture, then making a judgment call about whether the story's acceptance criteria, scope, and artifacts are coherent and complete. This is a *nuanced review and gatekeeping* task.[^1]

PO also does document sharding (`*shard-doc`) — taking a large PRD and splitting it into the per-epic chunks the SM and DEV will consume. This requires deep contextual understanding of the whole document to shard sensibly.[^1]

Again, GLM-5.2 wins here: it has the best large-document synthesis intelligence of the three models (1M context, highest open-weight GDPval-AA Elo), and the judgment tests confirm it produces more accurate decisions on policy-compliance-style evaluations — which is essentially what PO does with story validation.[^5][^6]

One practical note: SM and PO can share the same GLM-5.2 API endpoint/session in Claude Code or Cline since both roles are read-heavy and planning-focused — no need for separate model instances.

***

### DEV (Developer) → **Kimi K2.7-Code**

**Reasoning:** The DEV agent implements stories end-to-end: writing code, tests, handling multi-file refactors, following architecture patterns, and calling file-system/shell tools across an entire codebase session. This is the highest tool-call volume, multi-turn, long-session role in the BMAD dev cycle.[^1]

Kimi K2.7-Code was purpose-built for exactly this workload. Key fit signals:

- **Forced thinking + preserve_thinking**: reasoning context accumulates and persists across every turn in a coding session — critical for maintaining architectural consistency across a multi-file story implementation[^8][^7]
- **MCP Mark Verified 81.1%** and **MCP-Atlas 76.0%** — the highest tool-use accuracy of the three models across real-world MCP environments (GitHub, Filesystem, Postgres, Playwright)[^8]
- **30% fewer thinking tokens** than K2.6 — means longer sessions before hitting cost/latency walls on complex story implementations[^12]
- **Long-horizon coding coverage**: explicitly trained on production incident resolution, open-source contributions, backend services, infrastructure, ML/data, security, and multi-file refactors — the exact taxonomy of stories a BMAD DEV agent receives[^8]

GLM-5.2 is competitive on coding (Terminal-Bench 81.0, SWE-bench Pro 62.1) but is **token-hungry** (~43K output tokens/task vs Kimi's improved efficiency). In a long DEV session implementing a full epic, GLM-5.2's token burn rate would significantly increase cost and latency. Kimi K2.7's native coding-agent optimization, forced thinking mode, and MCP-first design make it the right pick for the DEV chair.[^5]

DeepSeek V4 Pro is strongest on algorithmic/competitive coding (LiveCodeBench 93.5, Codeforces 3206) but weaker on the agentic tool-orchestration side (MCP-Atlas 73.6 vs Kimi's 76.0). For stories that are computation-logic-heavy or involve algorithmic optimization, V4 Pro is a strong DEV substitute or pair.[^19][^4]

***

### QA (Quality Assurance) → **DeepSeek V4 Pro**

**Reasoning:** The QA agent performs risk-based review, test scenario design, requirements-to-test tracing (Given-When-Then), and quality gate decisions. This role demands:[^1]

- Deep analytical reasoning about what could go wrong in the implementation
- Ability to trace requirements back to test cases with precision
- Adversarial thinking — identifying edge cases, race conditions, boundary violations
- Strong pure-logic reasoning to evaluate whether test coverage is actually sufficient

DeepSeek V4 Pro's **LiveCodeBench 93.5** and **Codeforces CodeElo 3206** are the best indicators here — they measure the model's ability to reason about code correctness against precise specifications under pressure, which is exactly what QA does when reviewing story implementations. Its architecture explicitly supports tool calls inside reasoning steps, so it can actively probe a story file's code by running tests or searching for patterns while building its review.[^20][^21][^19]

DeepSeek V4 Pro also holds a **$0.87/M output price** advantage — the QA role generates high-volume review output (test scenarios, trace maps, risk assessments) across every story in a sprint, and running this at 5× lower cost than GLM-5.2 is meaningful over a full project.[^15]

The code review signal from community benchmarks also supports V4 Pro: in the May 2026 open-weight code review automation matchup, DeepSeek V4 Pro was rated the cost-quality winner for automated code review tasks.[^22]

***

### Party Mode (Brainstorming / Hashing Out) → **GLM-5.2** (as Party Orchestrator)

BMAD Party Mode is invoked via `*party-mode` from any agent or the orchestrator. It loads all installed agents, and the BMad Master picks 2-3 relevant personas per message to respond in character — the goal is cross-pollination of ideas across PM, Architect, UX, Analyst, DEV, QA, and others.[^23][^24]

Party Mode is used for: post-sprint debriefs, big architectural decisions with trade-offs, brainstorming sessions, failure post-mortems, and sprint retrospectives. The model hosting Party Mode needs:[^25][^23]

- **Highest conversational intelligence** — to impersonate multiple distinct expert personas convincingly and coherently
- **Broad knowledge depth across domains** — PM, architecture, UX, security, business analysis simultaneously
- **Nuanced judgment** — to let agents genuinely disagree and offer non-obvious perspectives
- **Long enough context** to hold the entire party conversation as it grows

GLM-5.2 is the best fit here for all four reasons. Its GDPval-AA score of 1524 (measuring multi-domain agentic knowledge work) leads the open-weight field. In the blind judgment tests, GLM-5.2's average of 80.7 vs V4 Pro's 69.7 on strategy/multi-team/conflicting-data scenarios directly predicts which model will produce richer, more actionable Party Mode discussions.[^6][^5]

Kimi K2.7-Code is too narrowly optimized for code execution to host a nuanced multi-persona brainstorming session. DeepSeek V4 Pro would be a reasonable fallback if GLM-5.2 API access is limited, given its strong reasoning, but its lower planning-judgment scores would show in shallower persona voices.[^6]

***

## Full Role-Model Assignment Matrix

| BMAD Agent | Recommended Model | Reasoning Mode | Key Fit Signals |
|------------|------------------|----------------|-----------------|
| **SM** | GLM-5.2 | Max thinking | 1M context; GDPval 1524; structured doc synthesis; 80.7 judgment score |
| **PO** | GLM-5.2 | High or Max | 1M context; story validation = judgment-heavy gatekeeping; best planning quality |
| **DEV** | Kimi K2.7-Code | Forced thinking (always on) | MCP Mark 81.1%; preserve_thinking; 30% token efficiency; built for agentic long-horizon coding |
| **QA** | DeepSeek V4 Pro | Max thinking | LiveCodeBench 93.5; Codeforces 3206; code-review accuracy; $0.87/M cost efficiency |
| **Party Mode** | GLM-5.2 | High (conversational) | GDPval 1524; multi-domain depth; 80.7 vs 69.7 judgment gap; best persona diversity |

***

## Benchmark Reference: Side-by-Side

| Benchmark | GLM-5.2 | Kimi K2.7-Code | DeepSeek V4 Pro | What It Measures |
|-----------|---------|----------------|-----------------|-----------------|
| AA Intelligence Index v4.1 | **51** | ~41.9 | 44 | Overall intelligence (open-weight leader) |
| SWE-bench Pro | **62.1%** | ~55 (est.) | 55.4% | Long-horizon repo-level code fixes |
| FrontierSWE | **74.4%** | — | 29.0% | Frontier-level complex SW engineering |
| Terminal-Bench 2.1 | **81.0** | — | 64.0 | Agentic shell + long-horizon dev tasks |
| MCP-Atlas (tool use) | 77.0 | **76.0** | 73.6 | Real MCP tool orchestration |
| MCP Mark Verified | — | **81.1%** | — | Verified tool-call accuracy |
| LiveCodeBench | — | — | **93.5** | Competitive algo/logic coding |
| Codeforces CodeElo | — | — | **3206** | Algorithmic reasoning vs humans |
| GDPval-AA (knowledge work) | **1524** | — | 1328 | Multi-domain agentic knowledge work |
| Judgment tests (blind) | **80.7** | — | 69.7 | Strategic planning / policy / allocation |
| Context window | **1M** | 256K | **1M** | Max usable context |
| Output price / 1M tokens | $4.40 | $3.07 | **$0.87** | Per-agent cost efficiency |

*Sources: Artificial Analysis, Hugging Face model cards, Codersera, MarkTechPost, community benchmarks*[^11][^18][^15][^7][^5][^6][^4]

***

## When to Break the Rules

The assignments above are optimized for a *balanced ongoing project* with an existing PRD and architecture. There are cases where you'd swap:

- **DEV → DeepSeek V4 Pro**: when a story is heavily algorithmic (sorting/indexing logic, crypto, data structure optimization). V4 Pro's Codeforces 3206 ranking makes it the best open-weight model for this type of work.[^19]
- **QA → GLM-5.2**: when the QA review involves large documentation review (reading the entire PRD + arch + story simultaneously to verify traceability). V4 Pro's 55.4% SWE-bench Pro is lower than GLM-5.2's 62.1% for complex holistic reviews.[^4]
- **Party Mode → DeepSeek V4 Pro**: cost-controlled brainstorming for lower-stakes design decisions where budget matters more than peak persona quality.[^6]
- **SM/PO → DeepSeek V4 Pro**: small brownfield tasks where the spec is already sharded and stories are already drafted — the judgment delta matters less when context is pre-loaded and small.[^16]

***

## Implementation Notes for a Multi-Model BMAD Stack

Since all three models expose OpenAI-compatible or Anthropic-compatible API endpoints, the simplest multi-model BMAD setup in Claude Code is:[^21][^3][^7]

```json
// ~/.claude/settings.json (simplified)
{
  "env": {
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.2[1m]",     // SM / PO / Party Mode
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "kimi-k2.7-code", // DEV
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "deepseek-v4-pro"  // QA / high-volume tasks
  }
}
```

This wires the highest-intelligence Claude Code slot (Opus) to GLM-5.2 for planning/orchestration, the mid-tier slot (Sonnet) to Kimi K2.7-Code for implementation, and the high-efficiency slot (Haiku) to DeepSeek V4 Pro for QA and volume tasks. BMAD then naturally routes each agent to its appropriate model tier based on its internal complexity signals.

For direct API orchestration (Python agents, n8n flows, etc.), each model is reachable at:
- **GLM-5.2**: `https://api.z.ai/api/coding/paas/v4` (Anthropic-compatible)[^3]
- **Kimi K2.7-Code**: `https://api.moonshot.cn/v1` (OpenAI-compatible)[^26]
- **DeepSeek V4 Pro**: `https://api.deepseek.com/v1` (OpenAI-compatible)[^21]

---

## References

1. [Agent Reference | bmad-code-org/BMAD-METHOD | DeepWiki](https://deepwiki.com/bmad-code-org/BMAD-METHOD/4-agent-reference) - This document provides a comprehensive reference for all agents available in the BMAD-METHOD framewo...

2. [Agent Roles in BMad Method](https://docs.bmad-method.org/explanation/core-concepts/agent-roles/) - Understanding the different agent roles in BMad Method

3. [GLM-5.2: Features, Setup, Benchmarks, and Model Switching Guide](https://www.datacamp.com/blog/glm-5-2) - In standard coding benchmarks, GLM-5.2 significantly outperforms its predecessor. It achieved an 81....

4. [unsloth/GLM-5.2](https://huggingface.co/unsloth/GLM-5.2) - We’re on a journey to advance and democratize artificial intelligence through open source and open s...

5. [GLM-5.2: The Complete Guide (2026)](https://codersera.com/blog/glm-5-2-complete-guide-2026/) - Z.ai's GLM-5.2 is the leading open-weights LLM on the Artificial Analysis Intelligence Index v4.1. 7...

6. [Blind head-to-head: GLM-5.2 vs DeepSeek V4 Pro on 3 real decisions](https://www.reddit.com/r/DeepSeek/comments/1u9zeqn/blind_headtohead_glm52_vs_deepseek_v4_pro_on_3/) - Blind head-to-head: GLM-5.2 vs DeepSeek V4 Pro on 3 real decisions

7. [Kimi K2.7-Code – 262k context, multimodal, open source](https://www.llmreference.com/model/kimi-k2-7-code) - Kimi K2.7-Code is Moonshot AI's coding-focused multimodal model released June 12, 2026, built on Kim...

8. [Kimi K2.7 Code API](https://www.together.ai/models/kimi-k27-code) - 1T parameter (32B activated) coding-focused agentic model with 30% fewer thinking tokens vs K2.6, im...

9. [Kimi K2.7 Review: Benchmarks, Coding, and Local ... - Flowtivity](https://flowtivity.ai/blog/kimi-k2-7-complete-review/) - Kimi Code Bench v2 (in-house coding benchmark): K2.7 scored 62.0, up 21.8% from K2.6's 50.9. GPT-5.5...

10. [Kimi K2.7 Code Cuts Reasoning Costs by 30% - Truefoundry](https://www.truefoundry.com/blog/kimi-k2-7-code-cuts-reasoning-costs-by-30----and-beats-claude-opus-4-8-on-mcp-tool-use)

11. [Kimi K2.7 Code model - NanoGPT](https://nano-gpt.com/models/text/moonshotai/kimi-k2.7-code)

12. [The Chinese-made AI model 'Kimi K2.7 Code' has been released as an open model, boasting the best coding performance in the Kimi series and low token consumption.](https://gigazine.net/gsc_news/en/20260615-kimi-k2-7-code/) - The news blog specialized in Japanese culture, odd news, gadgets and all other funny stuffs. Updated...

13. [Open Weights Coding Model That Actually Competes With Claude](https://www.youtube.com/watch?v=qtTdNfY_ZRg) - Reviewing GLM 5.2, the new coding model from z.ai. See how its 1 million token context and Terminal ...

14. [DeepSeek V4: Open-Weight Frontier Reasoning at One-Sixth the Price](https://www.frankx.ai/blog/deepseek-v4-analysis-2026) - DeepSeek shipped V4-Pro (1.6T/49B active) and V4-Flash (284B/13B active) on April 24, 2026 under MIT...

15. [DeepSeek V4-Pro Review: Benchmarks, Pricing, Verdict ...](https://codersera.com/blog/deepseek-v4-pro-review-benchmarks-pricing-2026/) - Honest review of DeepSeek V4-Pro. Permanent $0.435/$0.87 pricing since 2026-05-22, 80.6% SWE-bench V...

16. [DeepSeek V4 Release: Features & Benchmarks 2026 - AgDex](https://agdex.ai/blog/deepseek-v4-release-2026) - Everything about the DeepSeek V4 release in 2026. New features, benchmark results, pricing, context ...

17. [DeepSeek V4 Review 2026: Benchmarks, Pricing & Distillation Controversy - AI Workflows](https://aiworkflows.tools/blog/deepseek-v4-review-benchmarks-pricing-2026) - DeepSeek V4-Pro scores 80.6% on SWE-bench at $3.48/M output — 87% cheaper than GPT-5.4. Full benchma...

18. [GLM-5.2 vs DeepSeek V4 Pro - CodingFleet](https://codingfleet.com/blog/glm-5-2-vs-deepseek-v4-pro/) - GLM-5.2 (62.1% Pro, $4.40) vs DeepSeek V4 Pro (55.4%, $0.87). GLM leads SWE. DeepSeek leads algorith...

19. [Mapping the DeepSeek V4 Evaluation Suite: A Field Guide to ...](https://redreamality.com/blog/deepseek-v4-benchmarks-guide/) - A complete walkthrough of every benchmark DeepSeek V4 reports against — from LiveCodeBench to SWE-be...

20. [DeepSeek V4 Pro API - Together AI](https://www.together.ai/models/deepseek-v4-pro) - 1.6T parameter (49B activated) MoE model with 1M token context, hybrid attention requiring only 27% ...

21. [DeepSeek V4 Pro by DeepSeek on Vercel AI Gateway, Specs ...](https://vercel.com/ai-gateway/models/deepseek-v4-pro/faq) - Common questions and answers about this model on AI Gateway. DeepSeek V4 Pro is DeepSeek's April 23,...

22. [Code review automation in 2026: Open-source frontier matchup ...](https://callsphere.ai/blog/llm-comparison-code-review-automation-open-vs-open-may-2026)

23. [Party Mode: Multi-Agent Collaboration | BMAD Method](http://docs.bmad-method.org/explanation/features/party-mode/)

24. [BMad Method · 15/15 · Advanced Elicitation and Party Mode](https://www.youtube.com/watch?v=J0VH7fRtosg) - Never ask an AI to 'make it better.' Force it to use structured reasoning—or watch multiple agents d...

25. [The Best Open Source LLMs for Coding Right Now (June ...](https://dev.to/zyvop/the-best-open-source-llms-for-coding-right-now-june-2026-n10) - The open-source coding LLM leaderboard looked completely different in April than it does today. Mini...

26. [Model List - Kimi API Platform](https://platform.kimi.ai/docs/models) - High-Speed version of Kimi K2.7 Code model, with output speed of approximately 180 Tokens/s and up t...

