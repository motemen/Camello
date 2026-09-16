# Postfix increment vs a prefix one opening an argument.
#
# `++` and `--` are prefix operators and postfix ones both, and a bareword call
# written without parentheses is where the two readings disagree about the whole
# statement: `foo $x++` is a call on a postfix increment, while `print $fh ++$x`
# is a filehandle followed by a prefix one. camello decides from how the
# operator was written — glued to the operand after it opens an argument,
# anything else closes the term in front of it (the parser contract).

# Postfix: a LIST_CALL_EXPR over a POSTFIX_EXPR, with no filehandle slot.
foo $x++;
foo $count--;
print $fh++;
print ${fh}--;

# Prefix: the scalar takes the filehandle slot and the increment opens the list.
print $fh ++$x;
print ${fh} ++$x;
print STDERR ++$errors;

# The same question a block call answers: `{}` is the block of a `(&@)` sub when
# a term follows it, and a glued `++` is such a term.
f {} ++$x;
f { $_ } --$x;
