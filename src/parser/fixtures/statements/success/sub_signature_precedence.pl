use feature 'signatures';

# Valid: || has higher precedence than assignment
sub f ($x = 1 || 2) {}
sub g ($x = $a || $b) {}

# Valid: && has higher precedence than assignment
sub h ($x = 1 && 2) {}

# Valid: parenthesized 'or' works
sub i ($x = (1 or 2)) {}
