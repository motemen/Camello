# Multi-level dereferencing patterns
@$$aaa;
%$$bbb;
$$$$ccc;
@$$$ddd;

# Mixed patterns
my @array = @$$ref;
my %hash = %$$hashref;
my $scalar = $$$$deepref;
