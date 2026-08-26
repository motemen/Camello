# perltidy's `#<<<` and `#>>>`: the lines between the markers, and the marker
# lines themselves, come back as they were written. A hand-aligned table is a
# thing the formatter cannot be told about any other way.
my $foo = 1;

#<<< a note may follow the marker
my @table = (
    [ 'a',    1     ],
    [ 'bbbb', 22    ],
    [ 'c',    333   ],
);
#>>>

my $bar = 2;

# The markers need not agree about where they sit on their lines.
my $baz = (
    #<<<
    'foo'   => 1,
    'bar'   => 22,
#>>>
);

# Outside a region nothing changes.
my %qux = (
    aaa => 1,
    b   => 2,
);

# A marker written inside a string is a string.
my $quux = <<'EOT';
#<<<
  still     text
#>>>
EOT

# A trailing marker is a trailing comment.
foo();    #<<< not on a line of its own

# An end with no beginning is ignored.
#>>>
my $corge = 3;
