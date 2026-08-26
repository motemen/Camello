# The `{ ... }` of a block-form dereference holds a block, not an expression, so
# it may end with `;` or with `,`.
my @a      = @{ get_ref(); };
my @b      = @{ get_ref(), };
my %by_key = map { $_ => 1 } @{ get_ref(); };
