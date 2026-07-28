$foo =~ /[a\/]/;
$foo =~ m{[/]};
$foo =~ /[]]/;
$foo =~ /foo
bar/x;
s/\//::/g;
m<.<a>.>;
my @list = qw(a (b) c);
