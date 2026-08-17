# perl ignores the rest of the `__DATA__` line; Net::DNS labels it. The text is
# not code and not trivia the formatter may move, so it belongs to the section.
my $x = 1;
__DATA__	## DEFAULT HINTS
; <<>> DiG 9.18.20 <<>> @b.root-servers.net . -t NS
