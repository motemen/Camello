# An empty list element. Perl allows every one of these and simply drops the
# element; they arrive in real code as leftover commas.
run(1,, 2);
my @list = ('a', 'b',, 'c');
run('key', => { op => 1 });
run(width =>, 1300);
run(sub { 1 },, 'message');
