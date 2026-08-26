sub alignment_examples {
    foo => $value;
    longer_key => compute_value();
    final_key => fetch_result();

    %hash = (
        a => 1,
        xxx => 2,
    );

    func(
        a => 1,
        xxx => 2,
    );

    say $foo if $debug;
    warn $error if $should_warn;

    notify $user unless $quiet;
    log_event $event unless $disabled;

    my $short = 1; # comment one
    my $long_variable_name = 2; # comment two
}

# The operator that supplies a default lines up too, `//` and `||` as one group
my $host = $args->{host} // 'localhost';
my $port = $o->{port} // 8080;
my $name = $o->{name} || 'anon';
my $tags = $o->{tags} // [];

# ... and a line without one ends the group
my $plain = 1;
my $other = $x // 2;
