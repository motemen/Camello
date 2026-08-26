# A newline after the opening brace indents the contents and brings the closing
# brace back to where the construct started (INDENT-2).
my @a = @{
    $self->list
};

my %h = %{
    $self->map
};

push @out, @{
    $x->{list}
}, 1;

for my $e (@{
    $self->items
}) {
    say $e;
}

my $flat  = ${ $self->ref };
my $caret = ${^MATCH};
