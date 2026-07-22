SHELL := /bin/bash

all:

develop:
	source ./.env.develop && docker compose -f docker-compose.yml -f docker-compose.develop.yml up --build

up:
	docker compose up -d --build

.PHONY: all develop up
