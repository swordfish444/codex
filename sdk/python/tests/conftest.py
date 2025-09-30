from __future__ import annotations

from collections.abc import AsyncIterator, Awaitable, Callable

import pytest
import pytest_asyncio
from pytest import MonkeyPatch

from .codex_exec_spy import CodexExecSpyResult, install_codex_exec_spy
from .responses_proxy import ResponsesProxy, ResponsesProxyOptions, start_responses_test_proxy

ProxyFactory = Callable[[ResponsesProxyOptions], Awaitable[ResponsesProxy]]
SpyFactory = Callable[[ResponsesProxy], CodexExecSpyResult]


@pytest_asyncio.fixture
async def make_responses_proxy() -> AsyncIterator[ProxyFactory]:
    proxies: list[ResponsesProxy] = []

    async def _make(options: ResponsesProxyOptions) -> ResponsesProxy:
        proxy = await start_responses_test_proxy(options)
        proxies.append(proxy)
        return proxy

    try:
        yield _make
    finally:
        for proxy in proxies:
            await proxy.close()


@pytest.fixture
def codex_exec_spy(monkeypatch: MonkeyPatch) -> SpyFactory:
    def _install(proxy: ResponsesProxy) -> CodexExecSpyResult:
        return install_codex_exec_spy(monkeypatch, proxy)

    return _install
