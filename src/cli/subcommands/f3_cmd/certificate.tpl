Instance:     {{ GPBFTInstance }}
Power Table:
  Next:       {{ power_table_cid }}
  Delta:      {{ power_table_delta_string }}
Finalized Chain:
  Length:     {{ ECChain | length }}
  Epochs:     {{ epochs }}
  Chain:
{% for line in chain_lines %}
  {{- line }}
{% endfor %}
