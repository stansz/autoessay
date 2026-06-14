"""Shared test fixtures for autoessay."""

import os
import tempfile
from pathlib import Path

import pytest


@pytest.fixture
def temp_config_dir():
    """Create a temporary config directory for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        os.environ["AUTOESSAY_HOME"] = tmpdir
        yield Path(tmpdir)


@pytest.fixture
def sample_style_profile():
    """Return a minimal StyleProfile for testing."""
    from autoessay.style.profile import StyleProfile

    return StyleProfile(
        name="test-profile",
        use_case="testing",
        key_traits=["concise", "direct"],
    )
