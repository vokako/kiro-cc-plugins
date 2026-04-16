"""Git source management and marketplace parsing."""

from __future__ import annotations

import json
import shutil
import subprocess
from datetime import datetime
from pathlib import Path

from .models import CACHE_DIR, CONFIG_FILE, Source, load_json, save_json

def _cache_dir_for(name: str) -> Path:
    """Map source name (e.g. 'anthropics/skills') to a cache directory."""
    return CACHE_DIR / name.replace("/", "--")


EXTERNAL_CACHE = CACHE_DIR / "_external"


def _run(cmd: list[str], cwd: str | Path | None = None) -> str:
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)
    if r.returncode != 0:
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\n{r.stderr}")
    return r.stdout.strip()


def _get_commit(repo_dir: Path) -> str:
    return _run(["git", "rev-parse", "HEAD"], cwd=repo_dir)


def add_source(url: str, name: str | None = None) -> Source:
    """Clone a marketplace/plugin repo and register it."""
    if name is None:
        # Extract owner/repo from URL
        path = url.rstrip("/").rstrip(".git")
        parts = path.split("/")
        if len(parts) >= 2:
            name = f"{parts[-2]}/{parts[-1]}"
        else:
            name = parts[-1]

    repo_dir = _cache_dir_for(name)
    if repo_dir.exists():
        _run(["git", "pull", "--ff-only"], cwd=repo_dir)
    else:
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        _run(["git", "clone", "--depth", "1", url, str(repo_dir)])

    src = Source(name=name, url=url, cloned_at=datetime.now().isoformat(), commit=_get_commit(repo_dir))

    config = load_json(CONFIG_FILE, {"sources": []})
    config["sources"] = [s for s in config["sources"] if s["name"] != name]
    config["sources"].append({"name": src.name, "url": src.url, "cloned_at": src.cloned_at, "commit": src.commit})
    save_json(CONFIG_FILE, config)
    return src


def update_source(name: str) -> str:
    """Git pull a cached source, return new commit."""
    repo_dir = _cache_dir_for(name)
    if not repo_dir.exists():
        raise FileNotFoundError(f"Source '{name}' not found in cache")
    _run(["git", "pull", "--ff-only"], cwd=repo_dir)
    commit = _get_commit(repo_dir)

    config = load_json(CONFIG_FILE, {"sources": []})
    for s in config["sources"]:
        if s["name"] == name:
            s["commit"] = commit
    save_json(CONFIG_FILE, config)
    return commit


def remove_source(name: str):
    """Remove a cached source."""
    repo_dir = _cache_dir_for(name)
    if repo_dir.exists():
        shutil.rmtree(repo_dir)
    config = load_json(CONFIG_FILE, {"sources": []})
    config["sources"] = [s for s in config["sources"] if s["name"] != name]
    save_json(CONFIG_FILE, config)


def list_sources() -> list[dict]:
    return load_json(CONFIG_FILE, {"sources": []}).get("sources", [])


def get_source_dir(name: str) -> Path:
    d = _cache_dir_for(name)
    if not d.exists():
        raise FileNotFoundError(f"Source '{name}' not cached. Run: kiro-cc source add <url>")
    return d


# ── external plugin fetching ─────────────────────────────────────────────

def _normalize_git_url(url: str) -> str:
    """Ensure url is a full git clone URL."""
    if url.startswith(("http://", "https://", "git@")):
        return url
    # shorthand like "owner/repo" → github https
    return f"https://github.com/{url}.git"


def fetch_external_plugin(plugin_name: str, source_spec: dict) -> Path | None:
    """Clone an external plugin and return its local path."""
    EXTERNAL_CACHE.mkdir(parents=True, exist_ok=True)
    dest = EXTERNAL_CACHE / plugin_name

    src_type = source_spec.get("source", "") if isinstance(source_spec, dict) else ""

    if src_type == "url":
        url = _normalize_git_url(source_spec["url"])
        ref = source_spec.get("ref")
        sha = source_spec.get("sha")
        return _clone_or_pull(dest, url, ref=ref, sha=sha)

    elif src_type == "github":
        repo = source_spec["repo"]
        url = f"https://github.com/{repo}.git"
        ref = source_spec.get("ref")
        sha = source_spec.get("sha")
        return _clone_or_pull(dest, url, ref=ref, sha=sha)

    elif src_type == "git-subdir":
        url = _normalize_git_url(source_spec["url"])
        subpath = source_spec.get("path", "")
        ref = source_spec.get("ref")
        sha = source_spec.get("sha")
        repo_dir = _clone_or_pull(dest, url, ref=ref, sha=sha)
        if repo_dir and subpath:
            full = repo_dir / subpath
            return full if full.is_dir() else None
        return repo_dir

    return None


def _clone_or_pull(dest: Path, url: str, ref: str | None = None, sha: str | None = None) -> Path | None:
    """Clone (or pull) a repo to dest. Optionally checkout ref/sha."""
    try:
        if dest.exists():
            _run(["git", "fetch", "--depth", "1", "origin"], cwd=dest)
            if sha:
                _run(["git", "fetch", "--depth", "1", "origin", sha], cwd=dest)
                _run(["git", "checkout", sha], cwd=dest)
            elif ref:
                _run(["git", "checkout", f"origin/{ref}"], cwd=dest)
            else:
                _run(["git", "pull", "--ff-only"], cwd=dest)
        else:
            cmd = ["git", "clone", "--depth", "1"]
            if ref and not sha:
                cmd += ["--branch", ref]
            cmd += [url, str(dest)]
            _run(cmd)
            if sha:
                _run(["git", "fetch", "--depth", "1", "origin", sha], cwd=dest)
                _run(["git", "checkout", sha], cwd=dest)
        return dest
    except RuntimeError:
        return None


# ── marketplace parsing ──────────────────────────────────────────────────

def parse_marketplace(source_name: str) -> list[dict]:
    """Parse marketplace.json and return plugin entries with resolved local paths."""
    repo_dir = get_source_dir(source_name)
    mp_file = repo_dir / ".claude-plugin" / "marketplace.json"

    if not mp_file.exists():
        # Treat as single plugin
        pj = repo_dir / ".claude-plugin" / "plugin.json"
        name = source_name
        desc = ""
        if pj.exists():
            d = json.loads(pj.read_text())
            name = d.get("name", source_name)
            desc = d.get("description", "")
        return [{"name": name, "description": desc, "source": ".", "_local_path": str(repo_dir)}]

    mp = json.loads(mp_file.read_text())
    plugins = []
    for entry in mp.get("plugins", []):
        src = entry.get("source", "")
        local_path = None

        if isinstance(src, str) and src.startswith("."):
            local_path = str(repo_dir / src.lstrip("./"))
        elif isinstance(src, dict):
            # Check if already fetched
            ext_path = _resolve_external_cached(entry["name"], src)
            if ext_path:
                local_path = str(ext_path)

        plugins.append({
            "name": entry.get("name", ""),
            "description": entry.get("description", ""),
            "category": entry.get("category", ""),
            "source": src,
            "_local_path": local_path,
            "_skill_filter": entry.get("skills"),  # e.g. ["./skills/pdf", "./skills/xlsx"]
        })
    return plugins


def _resolve_external_cached(plugin_name: str, source_spec: dict) -> Path | None:
    """Check if an external plugin is already cached."""
    dest = EXTERNAL_CACHE / plugin_name
    if not dest.is_dir():
        return None
    src_type = source_spec.get("source", "")
    if src_type == "git-subdir":
        subpath = source_spec.get("path", "")
        if subpath:
            full = dest / subpath
            return full if full.is_dir() else None
    return dest
