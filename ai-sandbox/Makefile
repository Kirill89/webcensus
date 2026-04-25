IMAGE := webcensus-ai-sandbox-vm
HOME_DIR := $(CURDIR)/..

.PHONY: build shell

.DEFAULT_GOAL := shell

build:
	$(MAKE) -C $(HOME_DIR) build
	docker build -t $(IMAGE) .

shell: build
	docker run --name $(IMAGE) --hostname $(IMAGE) --rm -it -v $(HOME_DIR):/root $(IMAGE)
