# Binary `-` and `+` vs a unary one opening an argument.
#
# After a scalar that is the first argument of a call without parentheses, perl
# reads a `-` or `+` as the start of a term only when it is spaced from the
# scalar and glued to what follows: `print $fh -1` prints `-1` to `$fh`, while
# `print $x - 1`, `print $x-1` and `print $x- 1` all print a difference.

# Binary: the scalar is an operand, and there is no filehandle slot.
my @r = head $limit - $n, @rest;
print $limit - $n, "\n";
print $limit-$n, "\n";
print $limit- $n, "\n";
unknownsub $limit + $n, @rest;
_find_next $idx+1, $tokens, $len;

# Unary: the scalar takes the filehandle slot and the sign opens the list.
print $fh -1;
print $fh +$n;
print $limit -$n, "\n";
autoflush $fh -1;
