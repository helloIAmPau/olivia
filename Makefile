SHELL := /bin/bash

all:

tools:
	docker build -t olivia/tools:0.0.0-dev ./tools

develop: tools
	source ./.env.develop && docker compose up --build

build: tools
	docker build -t olivia/harness:0.0.0-dev ./harness

.PHONY: all tools develop build
