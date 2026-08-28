use strict;
use warnings;
use Smart::Args::TypeTiny qw(args);

package Store;
use Smart::Args::TypeTiny qw(args);

# A repository whose own `delete` is meant to be reached through `->`.
sub delete {
    args my $class => 'ClassName',
        my $id     => 'Int';
    return $id;
}

sub want_strings {
    args my $class => 'ClassName',
        my $list   => 'ArrayRef[Str]';
    return $list;
}

sub want_rows {
    args my $class => 'ClassName',
        my $list   => 'ArrayRef[ArrayRef[Int]]';
    return $list;
}

sub want_count {
    args my $class => 'ClassName',
        my $n      => 'Int';
    return $n;
}

package main;

# `delete` is perl's, whatever `Store` calls its own subs: a `sub` in the
# package does not take a bareword call away from a builtin.
my %args = (title => 'x');
my $title = delete $args{title};
my $rows  = { a => 1 };
delete $rows->{a};

# Reached through the arrow, it is the sub again.
Store->delete(id => 1);
Store->delete;                  #~ error missing-argument: requires `id`

sub read_map {
    args my $map => 'HashRef[ArrayRef[Int]]';

    # In a list literal the context is written down: the keys of a hash are
    # strings and its values are what it holds.
    Store->want_strings(list => [ keys %$map ]);
    Store->want_rows(list => [ values %$map ]);
    Store->want_rows(list => [ keys %$map ]);
    #~ error type-mismatch: (`ArrayRef[Str]`) passed to `list`
    Store->want_strings(list => [ values %$map ]);
    #~ error type-mismatch: (`ArrayRef[ArrayRef[Int]]`) passed to `list`

    # `scalar` of a container is its count; of anything else it is whatever
    # that was in scalar context, which is what every type here already is.
    Store->want_count(n => scalar keys %$map);
    Store->want_count(n => scalar %$map);
    return $title;
}
