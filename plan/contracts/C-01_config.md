# C-01 - Funnel Config

funnel:
  max_size_bytes: 1048576
  max_sections: 100
  max_nesting_depth: 8
  max_revisions_retained: 100
  max_blocks_per_section: 50

compat:
  supported_versions: [1]
  deprecated_versions: []
  unsupported_versions: []

publication:
  atomic: true
  conditional_on_revision: true
