use feature 'signatures';

# Invalid: 'or' has lower precedence than assignment
sub f ($x = 1 or 2) {}

# Invalid: 'and' has lower precedence than assignment
sub g ($x = 1 and 2) {}
