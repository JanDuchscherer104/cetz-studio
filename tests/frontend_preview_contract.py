"""Small source-level guard for the agreed frontend preview contract."""
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = (ROOT / "web/app.js").read_text(encoding="utf-8")
CONTROLS = (ROOT / "web/src/controls.js").read_text(encoding="utf-8")


def test_preview_contract_and_debounced_inputs() -> None:
    assert "fetch('/api/preview'" in APP
    assert "fetch('/api/preview/status'" in APP
    assert "fetch('/api/preview/cancel'" in APP
    assert "session_id:app.sessionId,revision:app.snapshot.revision,generation,command" in APP
    assert "state==='current'" in APP
    assert "state==='failed'" in APP
    assert "state==='cancelled'" in APP
    assert "setTimeout(emitPreview, 200)" in CONTROLS
    assert "setTimeout(()=>preview(command(),transaction),400)" in APP
    assert "Apply to adopt" in APP
    assert "showing last valid" in APP
