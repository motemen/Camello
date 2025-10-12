use feature 'signatures';

# Invalid: numbers as parameter names
sub f ($1) {}
sub g ($123) {}

# Valid: identifiers with digits
sub h ($foo123) {}
