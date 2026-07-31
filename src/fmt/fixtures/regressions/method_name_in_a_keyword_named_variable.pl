# `$obj->$name` takes the method name from a scalar. What follows the arrow is a
# variable, so the keyword the variable is named after has no meaning there:
# `$state`, `$sub` and `$if` are the same kind of thing as `$name` and `$format`,
# which already parse.
sub foo {
    my ($obj) = @_;

    my $name   = 'bar';
    my $format = 'baz';
    my $d      = $obj->$name;
    my $e      = $obj->$format;

    my $state = 'qux';
    my $sub   = 'quux';
    my $if    = 'corge';

    my $a = $obj->$state;
    my $b = $obj->$sub;
    my $c = $obj->$if;

    return ($a, $b, $c, $d, $e);
}
