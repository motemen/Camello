# Test cases distinguishing < operator from <GLOB>
# In function call context, < $h > should be comparison operators, not a glob

# These should parse as comparison operators in function arguments
foo < $h > 1_000;
bar < $b > $c;
baz < $x > 0;

# Comparison chains with barewords (should NOT be parsed as IO operator)
foo < h > bar;
test < a > b;

# Comparison with sub expression (should NOT be parsed as IO operator)
foo < $h > sub { 1 };

# Valid glob operators (for comparison)
my $fh = <STDIN>;
my @lines = <$filehandle>;
