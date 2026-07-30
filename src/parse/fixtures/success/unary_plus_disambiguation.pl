# Unary plus vs addition disambiguation.
#
# `ok +Foo->bar` is perl's documented disambiguating unary plus: it stops the
# argument from being read as a hash subscript or a block, so the `+` opens an
# argument list. `PI + 1` is addition applied to a constant. camello has no
# symbol table and cannot tell the two names apart, so it decides from how the
# operator was written — glued to its operand and spaced from the name in front
# of it is the idiom, anything else is arithmetic (ADR 0007 §6).

# The idiom: a PREFIX_EXPR inside the call's arguments.
ok +Foo::Bar->baz('/x');
is +{ a => 1 }, $expected;
is +Foo::Bar->qux($x), 1;

# Arithmetic: a BINARY_EXPR whose left side is the argument-less call.
my $limit = PI + 1;
my $tight = PI+1;
my $minus = MAX -1;

# A glued `+` in front of a plain variable is the idiom's shape too, and reads
# as an argument. Either way round the operand is written the same, so the
# reading is only visible in the tree.
report +$value;
