"""Tests for autoessay.provider — LLM abstraction layer."""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import Mock, patch

import httpx
import pytest
import yaml

from autoessay.provider import Provider, ProviderRegistry, get_registry

# ═══════════════════════════════════════════════════════════════════════════
# Helpers
# ═══════════════════════════════════════════════════════════════════════════

MINIMAL_PROVIDERS_YAML: dict = {
    "providers": {
        "deepseek": {
            "base_url": "https://api.deepseek.com/v1",
            "env_key": "DEEPSEEK_API_KEY",
            "models": {"fast": "deepseek-chat", "smart": "deepseek-reasoner"},
        },
        "anthropic": {
            "base_url": "https://api.anthropic.com/v1",
            "env_key": "ANTHROPIC_API_KEY",
            "models": {
                "fast": "claude-sonnet-4-20250514",
                "smart": "claude-opus-4-20250514",
            },
        },
    },
    "roles": {
        "drafting": "fast",
        "evaluation": "fast",
        "research": "smart",
    },
}


def _write_yaml(content: dict | None = None) -> Path:
    """Write a temporary providers.yaml and return its path."""
    tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
    yaml.dump(content or MINIMAL_PROVIDERS_YAML, tmp)
    tmp.close()
    return Path(tmp.name)


def _registry(env: dict[str, str] | None = None, config: dict | None = None) -> ProviderRegistry:
    """Create a ProviderRegistry with controlled env."""
    path = _write_yaml(config or MINIMAL_PROVIDERS_YAML)
    return ProviderRegistry(path, _env=env or {})


def _mock_http(content: str, model: str, provider: str) -> Mock:
    """Build a mock httpx.Response for either API type."""
    resp = Mock(spec=httpx.Response)
    resp.status_code = 200
    if provider == "anthropic":
        resp.json.return_value = {
            "model": model,
            "content": [{"text": content, "type": "text"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
    else:
        resp.json.return_value = {
            "model": model,
            "choices": [
                {"message": {"content": content, "role": "assistant"}, "finish_reason": "stop"}
            ],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5},
        }
    return resp


# ═══════════════════════════════════════════════════════════════════════════
# ProviderRegistry
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderRegistry:
    def test_loads_providers(self):
        reg = _registry()
        assert "deepseek" in reg.providers
        assert "anthropic" in reg.providers
        assert reg.providers["deepseek"].api_type == "openai"
        assert reg.providers["anthropic"].api_type == "anthropic"

    def test_roles_loaded(self):
        reg = _registry()
        assert reg.tier_for_role("drafting") == "fast"
        assert reg.tier_for_role("research") == "smart"
        assert reg.tier_for_role("nonexistent") == "fast"

    def test_available_empty_when_no_keys(self):
        reg = _registry(env={})
        assert reg.available == []

    def test_available_detects_set_keys(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-test"})
        assert reg.available == ["deepseek"]

    def test_get_unknown_provider_raises(self):
        reg = _registry()
        with pytest.raises(KeyError, match="Unknown provider"):
            reg.get("openai")

    def test_resolve_provider_prefer(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-ds", "ANTHROPIC_API_KEY": "sk-ant"})
        cfg, model = reg.resolve_provider("smart", prefer="anthropic")
        assert cfg.name == "anthropic"
        assert model == "claude-opus-4-20250514"

    def test_resolve_provider_exclude(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-ds", "ANTHROPIC_API_KEY": "sk-ant"})
        cfg, model = reg.resolve_provider("fast", exclude="deepseek")
        assert cfg.name == "anthropic"

    def test_resolve_provider_exclude_all_raises(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-ds"})
        with pytest.raises(RuntimeError, match="excluding 'deepseek'"):
            reg.resolve_provider("fast", exclude="deepseek")

    def test_resolve_provider_no_available_raises(self):
        reg = _registry(env={})
        with pytest.raises(RuntimeError, match="No available providers"):
            reg.resolve_provider("fast")

    def test_prefer_unavailable_falls_back(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-ds"})
        cfg, model = reg.resolve_provider("fast", prefer="anthropic")
        assert cfg.name == "deepseek"

    def test_missing_config_raises(self):
        with pytest.raises(FileNotFoundError):
            ProviderRegistry(Path("/nonexistent/providers.yaml"), _env={})

    def test_invalid_yaml_raises(self):
        """Malformed YAML that parses but has no 'providers' key."""
        tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        tmp.write("other_key:\n  - item1\n")
        tmp.close()
        with pytest.raises(ValueError, match="missing 'providers'"):
            ProviderRegistry(Path(tmp.name), _env={})


# ═══════════════════════════════════════════════════════════════════════════
# Provider — OpenAI-compatible
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderOpenAICompatible:
    @pytest.fixture
    def reg(self):
        return _registry(env={"DEEPSEEK_API_KEY": "sk-test"})

    @pytest.fixture
    def provider(self, reg):
        return Provider(reg)

    def test_chat_routes_to_openai_format(self, provider):
        mock = _mock_http("Hello from DeepSeek", "deepseek-chat", "deepseek")
        with patch.object(provider._get_client(), "post", return_value=mock) as post:
            resp = provider.chat([{"role": "user", "content": "Hi"}], tier="fast")

        assert post.call_args[0][0] == "https://api.deepseek.com/v1/chat/completions"
        sent = post.call_args[1]["json"]
        assert sent["model"] == "deepseek-chat"
        assert sent["messages"][0] == {"role": "user", "content": "Hi"}
        assert post.call_args[1]["headers"]["Authorization"] == "Bearer sk-test"
        assert resp.content == "Hello from DeepSeek"
        assert resp.model == "deepseek-chat"
        assert resp.provider == "deepseek"

    def test_system_prompt_inserted_as_message(self, provider):
        mock = _mock_http("ok", "deepseek-chat", "deepseek")
        with patch.object(provider._get_client(), "post", return_value=mock) as post:
            provider.chat(
                [{"role": "user", "content": "Hi"}],
                system="You are helpful.",
                tier="fast",
            )
        msgs = post.call_args[1]["json"]["messages"]
        assert msgs[0] == {"role": "system", "content": "You are helpful."}

    def test_last_provider_tracked(self, provider):
        mock = _mock_http("ok", "deepseek-chat", "deepseek")
        with patch.object(provider._get_client(), "post", return_value=mock):
            assert provider.last_provider is None
            provider.chat([{"role": "user", "content": "Hi"}])
            assert provider.last_provider == "deepseek"


# ═══════════════════════════════════════════════════════════════════════════
# Provider — Anthropic
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderAnthropic:
    @pytest.fixture
    def reg(self):
        return _registry(env={"ANTHROPIC_API_KEY": "sk-ant-test"})

    @pytest.fixture
    def provider(self, reg):
        return Provider(reg)

    def test_chat_routes_to_anthropic_format(self, provider):
        mock = _mock_http("Hello from Claude", "claude-sonnet-4-20250514", "anthropic")
        with patch.object(provider._get_client(), "post", return_value=mock) as post:
            resp = provider.chat([{"role": "user", "content": "Hi"}], tier="fast")

        assert post.call_args[0][0] == "https://api.anthropic.com/v1/messages"
        sent = post.call_args[1]["json"]
        assert sent["model"] == "claude-sonnet-4-20250514"
        assert sent["messages"][0] == {"role": "user", "content": "Hi"}
        assert post.call_args[1]["headers"]["x-api-key"] == "sk-ant-test"
        assert "Authorization" not in post.call_args[1]["headers"]
        assert resp.content == "Hello from Claude"
        assert resp.usage["prompt_tokens"] == 10
        assert resp.usage["completion_tokens"] == 5

    def test_system_prompt_top_level_for_anthropic(self, provider):
        mock = _mock_http("ok", "claude-sonnet-4-20250514", "anthropic")
        with patch.object(provider._get_client(), "post", return_value=mock) as post:
            provider.chat(
                [{"role": "user", "content": "Hi"}],
                system="You are Claude.",
                tier="fast",
            )
        sent = post.call_args[1]["json"]
        assert sent["system"] == "You are Claude."
        roles = [m["role"] for m in sent["messages"]]
        assert "system" not in roles


# ═══════════════════════════════════════════════════════════════════════════
# Provider — anti-self-review
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderAntiSelfReview:
    @pytest.fixture
    def reg(self):
        return _registry(env={"DEEPSEEK_API_KEY": "sk-ds", "ANTHROPIC_API_KEY": "sk-ant"})

    def test_draft_then_evaluate_uses_different_provider(self, reg):
        provider = Provider(reg)
        msgs = [{"role": "user", "content": "Write"}]

        # Draft with Anthropic
        with patch.object(
            provider._get_client(), "post",
            return_value=_mock_http("draft", "claude-sonnet-4-20250514", "anthropic"),
        ):
            draft = provider.chat(msgs, tier="fast", prefer="anthropic")
        assert draft.provider == "anthropic"
        assert provider.last_provider == "anthropic"

        # Evaluate: exclude anthropic → deepseek
        with patch.object(
            provider._get_client(), "post",
            return_value=_mock_http("eval", "deepseek-chat", "deepseek"),
        ) as post:
            eval_resp = provider.chat(msgs, tier="fast", exclude=provider.last_provider)
        assert eval_resp.provider == "deepseek"
        assert "deepseek" in post.call_args[0][0]

    def test_chat_for_role_resolves_tier(self, reg):
        provider = Provider(reg)
        # Explicitly prefer deepseek so we know which provider is used
        mock = _mock_http("result", "deepseek-reasoner", "deepseek")
        with patch.object(provider._get_client(), "post", return_value=mock) as post:
            provider.chat_for_role(
                [{"role": "user", "content": "Research"}],
                role="research",
                prefer="deepseek",
            )
        sent = post.call_args[1]["json"]
        assert sent["model"] == "deepseek-reasoner"  # research → smart tier


# ═══════════════════════════════════════════════════════════════════════════
# Provider — retry
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderRetry:
    @pytest.fixture
    def reg(self):
        return _registry(env={"DEEPSEEK_API_KEY": "sk-test"})

    def test_retries_on_429(self, reg):
        provider = Provider(reg)
        provider.RETRY_BASE_DELAY = 0

        err_429 = Mock(spec=httpx.Response)
        err_429.status_code = 429
        err_429.raise_for_status.side_effect = httpx.HTTPStatusError(
            "rate limited", request=Mock(), response=err_429
        )

        ok = _mock_http("finally ok", "deepseek-chat", "deepseek")

        with patch.object(
            provider._get_client(), "post", side_effect=[err_429, err_429, ok]
        ) as post:
            resp = provider.chat([{"role": "user", "content": "Hi"}])
        assert post.call_count == 3
        assert resp.content == "finally ok"

    def test_raises_after_max_retries(self, reg):
        provider = Provider(reg)
        provider.MAX_RETRIES = 1
        provider.RETRY_BASE_DELAY = 0

        err_503 = Mock(spec=httpx.Response)
        err_503.status_code = 503
        err_503.raise_for_status.side_effect = httpx.HTTPStatusError(
            "unavailable", request=Mock(), response=err_503
        )

        with patch.object(provider._get_client(), "post", return_value=err_503):
            with pytest.raises(httpx.HTTPStatusError):
                provider.chat([{"role": "user", "content": "Hi"}])

    def test_no_retry_on_400(self, reg):
        provider = Provider(reg)
        provider.RETRY_BASE_DELAY = 0

        err_400 = Mock(spec=httpx.Response)
        err_400.status_code = 400
        err_400.raise_for_status.side_effect = httpx.HTTPStatusError(
            "bad request", request=Mock(), response=err_400
        )

        with patch.object(provider._get_client(), "post", return_value=err_400) as post:
            with pytest.raises(httpx.HTTPStatusError):
                provider.chat([{"role": "user", "content": "Hi"}])
        assert post.call_count == 1


# ═══════════════════════════════════════════════════════════════════════════
# Provider — errors
# ═══════════════════════════════════════════════════════════════════════════


class TestProviderErrors:
    def test_missing_api_key_raises(self):
        # Set key initially, then remove it — resolve_provider sees no available
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-test"})
        provider = Provider(reg)
        # Override the env to simulate key disappearing
        reg._env = {}
        with pytest.raises(RuntimeError, match="No available providers"):
            provider.chat([{"role": "user", "content": "Hi"}])

    def test_chat_for_role_unknown_role_defaults_fast(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-test"})
        provider = Provider(reg)
        mock = _mock_http("ok", "deepseek-chat", "deepseek")
        with patch.object(provider._get_client(), "post", return_value=mock):
            resp = provider.chat_for_role(
                [{"role": "user", "content": "Hi"}], role="future_role"
            )
        assert resp.model == "deepseek-chat"


# ═══════════════════════════════════════════════════════════════════════════
# Convenience functions
# ═══════════════════════════════════════════════════════════════════════════


class TestConvenienceFunctions:
    def test_get_registry_returns_singleton(self):
        from autoessay import provider as pmod

        pmod._registry = None
        path = _write_yaml()
        r1 = get_registry(path)
        r2 = get_registry()
        assert r1 is r2

    def test_get_registry_new_path_creates_fresh(self):
        from autoessay import provider as pmod

        pmod._registry = None
        path1 = _write_yaml()
        path2 = _write_yaml({
            "providers": {
                "zai": {
                    "base_url": "https://api.z.ai/api/v1",
                    "env_key": "ZAI_API_KEY",
                    "models": {"fast": "glm-4-flash", "smart": "glm-4-plus"},
                }
            },
            "roles": {},
        })

        r1 = get_registry(path1)
        r2 = get_registry(path2)
        assert r1 is not r2
        assert "zai" in r2.providers
        assert "zai" not in r1.providers

    def test_close_cleans_up_client(self):
        reg = _registry(env={"DEEPSEEK_API_KEY": "sk-test"})
        provider = Provider(reg)
        assert provider._client is None
        _ = provider._get_client()
        assert provider._client is not None
        provider.close()
        assert provider._client is None
