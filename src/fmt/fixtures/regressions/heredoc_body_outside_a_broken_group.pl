# A group whose heredoc body lands *after* the statement cannot break: the body
# belongs to the line the marker is on, and a broken group ends several lines
# later. Specio::Library::Numeric writes this, and breaking it put the format
# string after the closing paren — a different program.
sub inline {
    return
        sprintf(
        <<'EOF', $_[0]->parent->inline_check( $_[1] ), ( $_[1] ) x 2 );
(
    %s
    &&
    %s >= -9
)
EOF
}
