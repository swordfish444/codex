**DOs**
- Bold the keyword: Gate tracing with env var: enable verbose output only when `CODEX_LOGIN_TRACE` is set.
```python
import os
CODEX_LOGIN_TRACE = os.environ.get("CODEX_LOGIN_TRACE", "false") in ("true", "1")
def trace(msg: str) -> None:
    if CODEX_LOGIN_TRACE:
        print(msg)
```
- Bold the keyword: Centralize request logic: wrap the probe in a tiny helper and always trace success/failure.
```python
import urllib.request

def attempt_request(method: str, context=None) -> bool:
    try:
        req = urllib.request.Request(f"{DEFAULT_ISSUER}/.well-known/openid-configuration", method="GET")
        with urllib.request.urlopen(req, context=context) as resp:
            if resp.status != 200:
                trace(f"Request using {method} failed: {resp.status}")
                return False
            trace(f"Request using {method} succeeded")
            return True
    except Exception as e:
        trace(f"Request using {method} failed: {e}")
        return False
```
- Bold the keyword: Try defaults first: attempt with Python’s default SSL settings before altering anything.
```python
CA_CONTEXT = None
ok = attempt_request("default settings", CA_CONTEXT)
```
- Bold the keyword: Prefer truststore when present: opportunistically use OS trust store without adding a hard dependency.
```python
if not ok:
    try:
        import truststore
        truststore.inject_into_ssl()  # patches default SSL to use OS store
        ok = attempt_request("truststore", CA_CONTEXT)  # context stays None on purpose
    except Exception as e:
        trace(f"Failed to use truststore: {e}")
```
- Bold the keyword: Fall back to certifi: create an explicit context from certifi’s CA bundle if needed.
```python
if not ok:
    try:
        import ssl, certifi
        CA_CONTEXT = ssl.create_default_context(cafile=certifi.where())
        ok = attempt_request("certifi", CA_CONTEXT)
    except Exception as e:
        trace(f"Failed to use certifi: {e}")
```
- Bold the keyword: Keep optional deps optional: import `truststore`/`certifi` lazily inside `try/except` and degrade gracefully.
```python
try:
    import truststore  # may not be installed
    truststore.inject_into_ssl()
except Exception as e:
    trace(f"Optional truststore unavailable: {e}")
```

**DON’Ts**
- Bold the keyword: Don’t assume third‑party deps exist: avoid unconditional, top‑level imports of `truststore`/`certifi`.
```python
# Avoid: crashes on machines without these packages
import truststore
import certifi
```
- Bold the keyword: Don’t spam logs: never print unguarded; always route through the trace helper.
```python
# Avoid
print("debug: connected")  # noisy without user intent
```
- Bold the keyword: Don’t treat non‑200 as success: explicitly check and trace unexpected statuses.
```python
# Avoid
with urllib.request.urlopen(req) as resp:
    return True  # ignores 4xx/5xx
```
- Bold the keyword: Don’t swallow errors silently: trace exceptions so users can diagnose TLS/store issues.
```python
# Avoid
except Exception:
    pass  # loses critical context for debugging
```
- Bold the keyword: Don’t modify SSL first: try default settings before injecting `truststore` or switching to `certifi`.
```python
# Avoid
import ssl, certifi
CA_CONTEXT = ssl.create_default_context(cafile=certifi.where())  # premature override
```