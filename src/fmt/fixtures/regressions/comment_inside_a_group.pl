# A comment runs to end of line, so a group holding one cannot be flat. Every
# case below used to concatenate the rest of the group onto the comment's line,
# which does not lay the code out badly — it comments it out.

# The opening delimiter's own line.
my %h = ( # keep me
    a => 1,
);

# A comment on an element, mid-list.
foo(1, # about the first
    2);

# A comment on the last element, with nothing but the closing delimiter left.
foo(1 # about the only one
);

# Anonymous array and anonymous hash, same rule.
my $list = [ # about the list
    1,
];
my $hash = { a => 1 # about a
};

# A comment on the line of the closing delimiter belongs to what comes after it,
# and must not be dragged inside.
my @xs = (1, 2); # about the whole thing

# Nested: the inner group holds the comment, and the outer one holds the inner.
outer(inner(1 # deep
), 2);

# A bare block after a commented statement. The comment belongs to the statement
# that ended, not to the brace that follows: claiming it moved it across a
# statement boundary and then emitted it twice.
my $y = 1; # about the assignment
{ # about the block
    inner();
}
