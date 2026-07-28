my $h = +{
    aaa => { xyz => '' },
    b   => { x   => '' },
};

map {
    +{
        time  => scalar @_,
        count => 2,
    };
} @list;
