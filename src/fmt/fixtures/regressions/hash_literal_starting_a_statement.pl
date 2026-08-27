# A `{` that opens a statement is a block, unless a `key =>` pair opens it or a
# subscript follows the matching `}` — then the whole thing was a hash literal,
# either returned or looked up in. The dispatch table written as the last
# expression of a sub is the common case, and read as a block its pairs were a
# wrapped expression statement, every one after the first a level deeper.
sub table {
    {
        foo => 'bar',
        bar => 'foo',
    }
}

# A key perl's own lookahead does not see is one camello does not act on: this
# is the block both of them read it as.
sub loop_once {
    {
        $k => 1,
    }
}

sub foo {
    my ($self) = @_;

    {
        bar => 'one',
        baz => 'two',
    }->{ $self->qux };
}

sub quux {
    my ($self) = @_;

    {
        bar => sub { return $self->id },
        baz => sub { return undef },
    }->{ $self->qux }->();
}
