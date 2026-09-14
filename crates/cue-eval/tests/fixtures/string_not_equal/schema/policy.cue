package schema

#Policy: {
    name: string & !="" & !="team-admin" & =~"^team-" & !~"-internal$"
}
