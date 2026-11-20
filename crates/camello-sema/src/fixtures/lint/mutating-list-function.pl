use strict;
use warnings;
use List::Util qw(first any);

my (@lines, $r, %h, $fh);
my @a = map { s/a/b/; $_ } @lines;          #~ warning mutating-list-function
my @b = grep { $_ =~ s/^x//g; 1 } @$r;      #~ warning mutating-list-function
my @c = map { $_ .= "!" } $r->@*;           #~ warning mutating-list-function
my @d = map { chomp; $_ } @{$r};            #~ warning mutating-list-function
my @e = map { tr/a-z/A-Z/; $_ } values %h;  #~ warning mutating-list-function
my $f = first { $_++; $_ > 2 } @lines;      #~ warning mutating-list-function
my $g = any { chop; /x/ } @_;               #~ warning mutating-list-function
sub h { map { s/a//; $_ } @_ }              #~ warning mutating-list-function
my @i = map { tr/a-c//d; $_ } (1, @lines); #~ warning mutating-list-function

my @ok1 = map { s/a/b/r } @lines;
my @ok2 = map { tr/a// } @lines;
my @ok3 = map { y/a/b/r } @lines;
my @ok4 = grep { chomp; length } <$fh>;
my @ok5 = map { s/a/b/; $_ } split /,/, "a,b";
my @ok6 = map { s/k//; $_ } keys %h;
my @ok7 = map { my $x = $_; $x =~ s/a/b/; $x } @lines;
my @ok8 = map { local $_ = $_; s/a/b/; $_ } @lines;
my @ok9 = map { [ map { s/a/b/; $_ } split // ] } @lines;
my @ok10 = map { s/a/b/ for my @copy = ($_); @copy } @lines;
my @ok11 = map { $_ + 1 } @lines;
map { s/a/b/ } @lines;
s/a/b/ for @lines;
my @ok12 = sort { $a cmp $b } @lines;
my @ok13 = map { chomp(my $x = $_); $x } @lines;
print @a, @b, @c, @d, @e, $f, $g, @i, @ok1, @ok2, @ok3, @ok4, @ok5, @ok6, @ok7,
    @ok8, @ok9, @ok10, @ok11, @ok12, @ok13;
@lines = map { s/a/b/; $_ } @lines;
@lines = grep { length } map { s/a/b/; $_ } @lines;
my $count = grep { s/^>// } @$r;
my $n = grep { $_ =~ s/^>// } @lines;
my @fresh = map { s/a/b/; $_ } @lines;      #~ warning mutating-list-function
my @kept = grep { s/a/b/; 1 } @lines;       #~ warning mutating-list-function
my @counts = map { s/a/b/ } @lines;         #~ warning mutating-list-function
print $count, $n, @fresh, @kept, @counts;
