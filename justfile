default:
	just --list

dev:
	cargo watch --clear --exec clippy --exec test
