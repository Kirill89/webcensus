IMAGE := webcensus
HOME_DIR := $(CURDIR)

.PHONY: build shell

.DEFAULT_GOAL := shell

build:
	mkdir -p data
	docker build -t $(IMAGE) .

shell: build
	docker run --name $(IMAGE) --hostname $(IMAGE) --rm -it -v $(HOME_DIR)/data:/root/data $(IMAGE)
