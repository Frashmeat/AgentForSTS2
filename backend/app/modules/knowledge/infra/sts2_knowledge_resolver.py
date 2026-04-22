from __future__ import annotations

from app.modules.knowledge.infra.sts2_code_facts_provider import Sts2CodeFactsProvider
from app.modules.knowledge.infra.sts2_guidance_provider import Sts2GuidanceProvider
from app.modules.knowledge.infra.sts2_lookup_provider import Sts2LookupProvider
from app.shared.contracts.knowledge import KnowledgePacket, KnowledgeQuery


class Sts2KnowledgeResolver:
    def __init__(
        self,
        code_facts_provider: Sts2CodeFactsProvider | None = None,
        guidance_provider: Sts2GuidanceProvider | None = None,
        lookup_provider: Sts2LookupProvider | None = None,
    ) -> None:
        self.code_facts_provider = code_facts_provider or Sts2CodeFactsProvider()
        self.guidance_provider = guidance_provider or Sts2GuidanceProvider()
        self.lookup_provider = lookup_provider or Sts2LookupProvider()

    def resolve(self, query: KnowledgeQuery) -> KnowledgePacket:
        facts, warnings = self.code_facts_provider.build_facts(query)
        guidance = self.guidance_provider.build_guidance(query)
        lookup = self.lookup_provider.build_lookup(query)
        summary = f"{query.domain}:{query.scenario}"
        return KnowledgePacket(
            domain=query.domain,
            scenario=query.scenario,
            summary=summary,
            facts=facts,
            guidance=guidance,
            lookup=lookup,
            warnings=warnings,
        )
