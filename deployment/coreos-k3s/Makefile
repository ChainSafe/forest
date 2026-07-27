SHELL := bash

.PHONY: all render jwt token clean
all: render

render:
	scripts/render.sh

jwt:
	@test -n "$(HOST)" || { echo "usage: make jwt HOST=core@<ip>"; exit 1; }
	scripts/push-jwt.sh "$(HOST)"

token:
	@test -n "$(HOST)" || { echo "usage: make token HOST=core@<ip> [SUB=name]"; exit 1; }
	scripts/mint-jwt.sh "$(HOST)" "$(if $(SUB),$(SUB),ci)"

clean:
	rm -rf out
