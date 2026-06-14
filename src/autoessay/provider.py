"""Provider abstraction layer — LLM API routing, auth, retries, rate limiting.

All LLM calls in autoessay go through this module. It provides:
- Provider auto-detection from environment variables
- Two-tier model selection (fast / smart)
- Anthropic native API + OpenAI-compatible API support
- Anti-self-review enforcement (different provider for evaluation)
- Exponential backoff retry on transient errors
- Standardized response objects
"""

from __future__ import annotations

import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import httpx
import yaml
from dotenv import load_dotenv

# ═══════════════════════════════════════════════════════════════════════════
# Data model
# ═══════════════════════════════════════════════════════════════════════════


@dataclass
class ProviderConfig:
    """Definition of a single LLM provider."""

    name: str
    base_url: str
    env_key: str
    models: dict[str, str]  # {"fast": "model-id", "smart": "model-id"}
    api_type: str = "openai"  # "openai" | "anthropic"


@dataclass
class LLMResponse:
    """Standardized response from any LLM provider."""

    content: str
    model: str
    provider: str
    usage: dict[str, int]  # {"prompt_tokens": N, "completion_tokens": N}
    finish_reason: str = "stop"


# ═══════════════════════════════════════════════════════════════════════════
# Registry — loads config, detects available providers
# ═══════════════════════════════════════════════════════════════════════════

_PROVIDER_YAML_PATHS = (
    Path("config/providers.yaml"),
    Path(__file__).resolve().parent.parent.parent / "config" / "providers.yaml",
)


class ProviderRegistry:
    """Loads provider definitions from YAML and detects available providers."""

    def __init__(
        self,
        config_path: Path | None = None,
        _env: dict[str, str] | None = None,
    ) -> None:
        load_dotenv(override=False)
        self._env = _env if _env is not None else os.environ
        self.config_path = config_path or self._find_config()
        self.providers: dict[str, ProviderConfig] = {}
        self._roles: dict[str, str] = {}  # role_name -> tier
        self._load()

    # ------------------------------------------------------------------
    # Config loading
    # ------------------------------------------------------------------

    @staticmethod
    def _find_config() -> Path:
        """Locate providers.yaml via env var, CWD, or package directory."""
        if env_path := os.getenv("AUTOESSAY_CONFIG"):
            return Path(env_path)
        for candidate in _PROVIDER_YAML_PATHS:
            if candidate.exists():
                return candidate
        raise FileNotFoundError(
            "providers.yaml not found. Set AUTOESSAY_CONFIG or run from repo root."
        )

    def _load(self) -> None:
        raw = yaml.safe_load(self.config_path.read_text())
        if not raw or "providers" not in raw:
            raise ValueError("Invalid providers.yaml: missing 'providers' key")

        for name, cfg in raw["providers"].items():
            self.providers[name] = ProviderConfig(
                name=name,
                base_url=cfg["base_url"],
                env_key=cfg["env_key"],
                models=cfg["models"],
                api_type="anthropic" if name == "anthropic" else "openai",
            )

        self._roles = raw.get("roles", {})

    # ------------------------------------------------------------------
    # Queries
    # ------------------------------------------------------------------

    @property
    def available(self) -> list[str]:
        """Provider names with API keys set in environment."""
        return [name for name, cfg in self.providers.items() if self._env.get(cfg.env_key)]

    def get(self, name: str) -> ProviderConfig:
        if name not in self.providers:
            raise KeyError(
                f"Unknown provider: '{name}'. Available: {list(self.providers)}"
            )
        return self.providers[name]

    def tier_for_role(self, role: str) -> str:
        """Return 'fast' or 'smart' for a pipeline role ('drafting', 'research', …)."""
        return self._roles.get(role, "fast")

    def resolve_provider(
        self,
        tier: str,
        *,
        prefer: str | None = None,
        exclude: str | None = None,
    ) -> tuple[ProviderConfig, str]:
        """Select an available provider + model for the given tier.

        Args:
            tier: ``"fast"`` or ``"smart"``.
            prefer: Preferred provider name (must be available).
            exclude: Provider to exclude — used to prevent self-review bias
                     during evaluation by excluding the drafting provider.

        Returns:
            ``(ProviderConfig, model_id)``

        Raises:
            RuntimeError: No provider is available for the requested tier.
        """
        available = [p for p in self.available if p != exclude]

        if not available:
            if exclude:
                msg = (
                    f"No providers for tier='{tier}' after excluding '{exclude}'. "
                    f"Set API keys for at least two providers to enable evaluation."
                )
            else:
                msg = (
                    f"No available providers for tier='{tier}'. "
                    "Set at least one provider API key."
                )
            raise RuntimeError(msg)

        # User-specified preference
        if prefer and prefer in available:
            cfg = self.get(prefer)
            model = cfg.models.get(tier, next(iter(cfg.models.values())))
            return cfg, model

        # Any available provider
        name = available[0]
        cfg = self.get(name)
        model = cfg.models.get(tier, next(iter(cfg.models.values())))
        return cfg, model


# ═══════════════════════════════════════════════════════════════════════════
# Provider — the main LLM interface
# ═══════════════════════════════════════════════════════════════════════════

RETRYABLE_STATUSES = frozenset({429, 500, 502, 503, 504})


class Provider:
    """Routes LLM calls to the right API with auth, retries, and rate limiting.

    Usage::

        reg = ProviderRegistry()
        p = Provider(reg)

        # Simple call using any available fast-tier model
        resp = p.chat(
            [{"role": "user", "content": "Summarize this…"}],
            tier="fast",
        )

        # Evaluation — exclude the drafting provider
        eval_resp = p.chat(
            messages,
            tier="fast",
            exclude=p.last_provider,  # anti-self-review
        )

        # By pipeline role (tier resolved from providers.yaml roles)
        resp = p.chat_for_role(messages, role="drafting")
    """

    MAX_RETRIES = 3
    RETRY_BASE_DELAY = 2  # seconds

    def __init__(self, registry: ProviderRegistry) -> None:
        self.registry = registry
        self._last_provider: str | None = None
        self._client: httpx.Client | None = None

    @property
    def last_provider(self) -> str | None:
        """Name of the provider used in the most recent call."""
        return self._last_provider

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def chat(
        self,
        messages: list[dict[str, str]],
        *,
        tier: str = "fast",
        prefer: str | None = None,
        exclude: str | None = None,
        system: str | None = None,
        max_tokens: int = 4096,
        temperature: float = 0.7,
    ) -> LLMResponse:
        """Send a chat completion request.

        Args:
            messages: List of ``{"role": "user", "content": "…"}`` dicts.
            tier: ``"fast"`` or ``"smart"`` — selects which model to use.
            prefer: Preferred provider name (uses auto-detected if omitted).
            exclude: Provider name to exclude (anti-self-review).
            system: System prompt.
            max_tokens: Maximum completion tokens.
            temperature: Sampling temperature (0–1).

        Returns:
            ``LLMResponse`` with content, model, provider, and token usage.
        """
        cfg, model = self.registry.resolve_provider(
            tier, prefer=prefer, exclude=exclude
        )

        api_key = self.registry._env.get(cfg.env_key)
        if not api_key:
            raise RuntimeError(
                f"Provider '{cfg.name}' requires {cfg.env_key} in environment"
            )

        result = self._retry(
            lambda: self._call(
                cfg, model, api_key, messages, system, max_tokens, temperature
            )
        )

        self._last_provider = cfg.name
        return result

    def chat_for_role(
        self,
        messages: list[dict[str, str]],
        *,
        role: str,
        prefer: str | None = None,
        exclude: str | None = None,
        system: str | None = None,
        max_tokens: int = 4096,
        temperature: float = 0.7,
    ) -> LLMResponse:
        """Send a chat request using the tier configured for a pipeline role.

        Args:
            role: Pipeline role name — ``"research"``, ``"outline"``,
                  ``"drafting"``, ``"evaluation"``, ``"revision"``, etc.
                  Tier is resolved from ``providers.yaml`` → ``roles``.
            Others: Same as :meth:`chat`.
        """
        tier = self.registry.tier_for_role(role)
        return self.chat(
            messages,
            tier=tier,
            prefer=prefer,
            exclude=exclude,
            system=system,
            max_tokens=max_tokens,
            temperature=temperature,
        )

    # ------------------------------------------------------------------
    # Internal — API routing
    # ------------------------------------------------------------------

    def _call(
        self,
        cfg: ProviderConfig,
        model: str,
        api_key: str,
        messages: list[dict[str, str]],
        system: str | None,
        max_tokens: int,
        temperature: float,
    ) -> LLMResponse:
        """Route to the correct API implementation."""
        if cfg.api_type == "anthropic":
            return self._call_anthropic(
                cfg, model, api_key, messages, system, max_tokens, temperature
            )
        return self._call_openai_compatible(
            cfg, model, api_key, messages, system, max_tokens, temperature
        )

    def _call_anthropic(
        self,
        cfg: ProviderConfig,
        model: str,
        api_key: str,
        messages: list[dict[str, str]],
        system: str | None,
        max_tokens: int,
        temperature: float,
    ) -> LLMResponse:
        """Native Anthropic Messages API call."""
        body: dict[str, Any] = {
            "model": model,
            "max_tokens": max_tokens,
            "messages": messages,
        }
        if system:
            body["system"] = system

        resp = self._get_client().post(
            f"{cfg.base_url}/messages",
            json=body,
            headers={
                "x-api-key": api_key,
                "anthropic-version": "2023-06-01",
                "Content-Type": "application/json",
            },
        )
        resp.raise_for_status()
        data = resp.json()

        return LLMResponse(
            content=data["content"][0]["text"],
            model=data["model"],
            provider=cfg.name,
            usage={
                "prompt_tokens": data["usage"]["input_tokens"],
                "completion_tokens": data["usage"]["output_tokens"],
            },
            finish_reason=data.get("stop_reason", "stop"),
        )

    def _call_openai_compatible(
        self,
        cfg: ProviderConfig,
        model: str,
        api_key: str,
        messages: list[dict[str, str]],
        system: str | None,
        max_tokens: int,
        temperature: float,
    ) -> LLMResponse:
        """OpenAI-compatible chat/completions call (DeepSeek, OpenRouter, Z.ai)."""
        msgs = list(messages)
        if system:
            msgs.insert(0, {"role": "system", "content": system})

        body: dict[str, Any] = {
            "model": model,
            "messages": msgs,
            "max_tokens": max_tokens,
            "temperature": temperature,
        }

        resp = self._get_client().post(
            f"{cfg.base_url}/chat/completions",
            json=body,
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
        )
        resp.raise_for_status()
        data = resp.json()

        choice = data["choices"][0]
        return LLMResponse(
            content=choice["message"]["content"],
            model=data["model"],
            provider=cfg.name,
            usage={
                "prompt_tokens": data["usage"]["prompt_tokens"],
                "completion_tokens": data["usage"]["completion_tokens"],
            },
            finish_reason=choice.get("finish_reason", "stop"),
        )

    # ------------------------------------------------------------------
    # Retry logic
    # ------------------------------------------------------------------

    def _retry(self, fn, max_retries: int | None = None) -> LLMResponse:
        """Exponential backoff on transient HTTP/network errors."""
        max_retries = max_retries or self.MAX_RETRIES
        last_exc: Exception | None = None

        for attempt in range(max_retries + 1):
            try:
                return fn()
            except httpx.HTTPStatusError as exc:
                if exc.response.status_code in RETRYABLE_STATUSES:
                    last_exc = exc
                    if attempt < max_retries:
                        time.sleep(self.RETRY_BASE_DELAY * (2**attempt))
                        continue
                raise
            except (httpx.ConnectError, httpx.TimeoutException) as exc:
                last_exc = exc
                if attempt < max_retries:
                    time.sleep(self.RETRY_BASE_DELAY * (2**attempt))
                    continue
                raise

        raise last_exc  # type: ignore[misc]

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def _get_client(self) -> httpx.Client:
        if self._client is None:
            self._client = httpx.Client(timeout=httpx.Timeout(120))
        return self._client

    def close(self) -> None:
        if self._client:
            self._client.close()
            self._client = None


# ═══════════════════════════════════════════════════════════════════════════
# Convenience singletons
# ═══════════════════════════════════════════════════════════════════════════

_registry: ProviderRegistry | None = None


def get_registry(config_path: Path | None = None) -> ProviderRegistry:
    """Return the default ``ProviderRegistry``, creating it on first call.

    Pass *config_path* to override the config file location on first creation.
    """
    global _registry
    if _registry is None or config_path is not None:
        _registry = ProviderRegistry(config_path)
    return _registry


def get_provider(config_path: Path | None = None) -> Provider:
    """Return a ``Provider`` backed by the default registry."""
    return Provider(get_registry(config_path))
