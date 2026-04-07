COQC        = coqc
COQFLAGS    = -R ../theories HelixDSL
OCAMLOPT    = ocamlfind ocamlopt
OCAMLFLAGS  = -package str -linkpkg

EXTRACTED_DIR = extracted
SENTINEL      = $(EXTRACTED_DIR)/Eval.ml

.PHONY: all runner clean

all: runner

# Step 1: extract Coq to OCaml.
# Requires the main project's `make` to have run first (theories/*.vo present).
$(SENTINEL): Extract.v
	@mkdir -p $(EXTRACTED_DIR)
	$(COQC) $(COQFLAGS) Extract.v
	@echo "Extraction complete — ML files in $(EXTRACTED_DIR)/"

# Step 2: compile all extracted ML files + the runner into the oracle binary.
# ocamldep -sort is used to get the correct compilation order (topological sort
# by module dependency), since the extracted files cannot be compiled in
# alphabetical order.  ocamlfind provides the `str` package.
runner: $(SENTINEL) runner.ml
	$(OCAMLOPT) $(OCAMLFLAGS) -I $(EXTRACTED_DIR) \
		$(shell ocamldep -sort -I $(EXTRACTED_DIR) $(EXTRACTED_DIR)/*.ml) \
		runner.ml \
		-o helix_eval
	@echo "Oracle binary built: ./helix_eval"

clean:
	rm -f *.vo *.glob *.vok *.vos *.vos.d .*.aux
	rm -f helix_eval
	rm -rf $(EXTRACTED_DIR)
