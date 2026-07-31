# A `{` that opens a statement is a block, unless a subscript follows the
# matching `}` — then the whole thing was a hash literal being looked up in. The
# dispatch table written as the last expression of a sub is the common case.
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
