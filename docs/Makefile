SPELLCHECK_DICTIONARY=./dictionary.txt
format-spellcheck-dictionary:
	@cat $(SPELLCHECK_DICTIONARY) | sort --ignore-case | uniq > $(SPELLCHECK_DICTIONARY).tmp
	@mv $(SPELLCHECK_DICTIONARY).tmp $(SPELLCHECK_DICTIONARY)

format-spellcheck-dictionary-check:
	@cat $(SPELLCHECK_DICTIONARY) | sort --ignore-case | uniq > $(SPELLCHECK_DICTIONARY).tmp
	@diff $(SPELLCHECK_DICTIONARY) $(SPELLCHECK_DICTIONARY).tmp
	@rm $(SPELLCHECK_DICTIONARY).tmp
