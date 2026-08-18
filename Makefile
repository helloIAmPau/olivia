SHELL := /bin/bash
.ONESHELL:

VERSION := $(shell git describe --tags --exact-match 2>/dev/null || git rev-parse --short HEAD)
export VERSION

# harness plus every tool crate discovered on disk (each tool is a
# `tools/<name>/Cargo.toml`; the `tools/Cargo.toml` workspace root is excluded).
MANIFESTS := harness/Cargo.toml $(wildcard tools/*/Cargo.toml)

all:

develop:
	source ./.env.develop && docker compose up --build

build:
	docker build -t olivia/harness:$(VERSION) .

bump:
	@set -euo pipefail
	level='$(filter-out $@,$(MAKECMDGOALS))'
	case "$$level" in
	  patch|minor|major) ;;
	  *) echo "usage: make bump <patch|minor|major>" >&2; exit 1 ;;
	esac
	if ! git diff-index --quiet HEAD --; then
	  echo "error: working tree has uncommitted changes; commit or stash first" >&2
	  exit 1
	fi
	current=$$(grep -m1 -E '^version[[:space:]]*=' harness/Cargo.toml | tr -dc '0-9.')
	if [[ -z "$$current" ]]; then echo "error: could not read current version" >&2; exit 1; fi
	IFS=. read -r major minor patch <<< "$$current"
	case "$$level" in
	  major) major=$$((major + 1)); minor=0; patch=0 ;;
	  minor) minor=$$((minor + 1)); patch=0 ;;
	  patch) patch=$$((patch + 1)) ;;
	esac
	next="$$major.$$minor.$$patch"
	tag="v$$next"
	if git rev-parse -q --verify "refs/tags/$$tag" >/dev/null; then
	  echo "error: tag $$tag already exists" >&2
	  exit 1
	fi
	echo "Bumping $$current -> $$next"
	for m in $(MANIFESTS); do
	  sed -i -E "s/^version[[:space:]]*=.*/version = \"$$next\"/" "$$m"
	done
	git add $(MANIFESTS)
	git commit -m "churn: bump version to $$tag"
	git tag "$$tag"
	echo "Committed and tagged $$tag"

patch minor major:
	@:

.PHONY: all develop build bump patch minor major
