# NW-A55 Analysis Pipeline
# Run inside WSL2 Ubuntu. See CLAUDE.md for full environment setup.
# Usage: make check-deps && make phase1 ... phase7

ARTIFACTS := artifacts
ANALYSIS  := analysis

SENTINEL_1 := $(ARTIFACTS)/.phase1.done
SENTINEL_2 := $(ARTIFACTS)/.phase2.done
SENTINEL_3 := $(ARTIFACTS)/.phase3.done
SENTINEL_4 := $(ARTIFACTS)/.phase4.done
SENTINEL_5 := $(ARTIFACTS)/.phase5.done
SENTINEL_6 := $(ARTIFACTS)/.phase6.done
SENTINEL_7 := $(ARTIFACTS)/.phase7.done

DEPS := git binwalk dtc file readelf strings nm qemu-arm-static cargo rustup clang

.PHONY: all check-deps phase1 phase2 phase3 phase4 phase5 phase6 phase7 clean

all: check-deps phase1 phase2 phase3 phase4 phase5 phase6 phase7

check-deps:
	@echo "=== Checking dependencies ==="
	@missing=""; \
	for dep in $(DEPS); do \
		if ! command -v $$dep >/dev/null 2>&1; then \
			printf "  MISSING: %s\n" "$$dep"; \
			missing="$$missing $$dep"; \
		else \
			printf "  OK:      %s\n" "$$dep"; \
		fi; \
	done; \
	if [ -n "$$missing" ]; then \
		echo ""; \
		echo "ERROR: missing deps:$$missing"; \
		echo "See CLAUDE.md Part B for install instructions."; \
		exit 1; \
	fi
	@echo "All dependencies present."

$(ARTIFACTS)/.phase1.done:
	@mkdir -p $(ARTIFACTS) $(ANALYSIS)
	bash phases/phase1.sh
	@touch $@

$(ARTIFACTS)/.phase2.done: $(ARTIFACTS)/.phase1.done
	bash phases/phase2.sh
	@touch $@

$(ARTIFACTS)/.phase3.done: $(ARTIFACTS)/.phase2.done
	bash phases/phase3_soc_id.sh
	@touch $@

$(ARTIFACTS)/.phase4.done: $(ARTIFACTS)/.phase3.done
	bash phases/phase4.sh
	@touch $@

$(ARTIFACTS)/.phase5.done: $(ARTIFACTS)/.phase4.done
	bash phases/phase5.sh
	@touch $@

$(ARTIFACTS)/.phase6.done: $(ARTIFACTS)/.phase5.done
	bash phases/phase6.sh
	@touch $@

$(ARTIFACTS)/.phase7.done: $(ARTIFACTS)/.phase6.done
	bash phases/phase7.sh
	@touch $@

phase1: $(ARTIFACTS)/.phase1.done
phase2: $(ARTIFACTS)/.phase2.done
phase3: $(ARTIFACTS)/.phase3.done
phase4: $(ARTIFACTS)/.phase4.done
phase5: $(ARTIFACTS)/.phase5.done
phase6: $(ARTIFACTS)/.phase6.done
phase7: $(ARTIFACTS)/.phase7.done

clean:
	rm -f $(ARTIFACTS)/.phase*.done
	@echo "Sentinels cleared. Re-run make phaseN to redo a phase."

clean-analysis:
	rm -rf $(ANALYSIS)/
	@echo "analysis/ cleared."

clean-all: clean clean-analysis
	@echo "Full clean done. artifacts/repos and firmware are preserved."
