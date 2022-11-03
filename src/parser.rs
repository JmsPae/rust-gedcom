//! The state machine that parses a char iterator of the gedcom's contents
use std::{panic, str::Chars};

use crate::tokenizer::{Token, Tokenizer};
use crate::tree::GedcomData;
use crate::types::{
    event::HasEvents, Address, Copyright, Corporation, CustomData, Date, Encoding, Event, Family,
    FamilyLink, GedcomDocument, Gender, HeadPlac, HeadSourData, HeadSource, Header, Individual,
    Name, Note, RepoCitation, Repository, Source, SourceCitation, Submitter, Translation,
};

/// The Gedcom parser that converts the token list into a data structure
pub struct Parser<'a> {
    tokenizer: Tokenizer<'a>,
}

impl<'a> Parser<'a> {
    /// Creates a parser state machine for parsing a gedcom file as a chars iterator
    #[must_use]
    pub fn new(chars: Chars<'a>) -> Parser {
        let mut tokenizer = Tokenizer::new(chars);
        tokenizer.next_token();
        Parser { tokenizer }
    }

    /// Does the actual parsing of the record.
    pub fn parse_record(&mut self) -> GedcomData {
        let mut data = GedcomData::default();
        loop {
            let level = match self.tokenizer.current_token {
                Token::Level(n) => n,
                _ => panic!(
                    "{} Expected Level, found {:?}",
                    self.dbg(),
                    self.tokenizer.current_token
                ),
            };

            self.tokenizer.next_token();

            let mut pointer: Option<String> = None;
            if let Token::Pointer(xref) = &self.tokenizer.current_token {
                pointer = Some(xref.to_string());
                self.tokenizer.next_token();
            }

            if let Token::Tag(tag) = &self.tokenizer.current_token {
                match tag.as_str() {
                    "HEAD" => data.header = self.parse_header(),
                    "FAM" => data.add_family(self.parse_family(level, pointer)),
                    "INDI" => data.add_individual(self.parse_individual(level, pointer)),
                    "REPO" => data.add_repository(self.parse_repository(level, pointer)),
                    "SOUR" => data.add_source(self.parse_source(level, pointer)),
                    "SUBM" => data.add_submitter(self.parse_submitter(level, pointer)),
                    "TRLR" => break,
                    _ => {
                        println!("{} Unhandled tag {}", self.dbg(), tag);
                        self.tokenizer.next_token();
                    }
                };
            } else if let Token::CustomTag(tag) = &self.tokenizer.current_token {
                // TODO
                let tag_clone = tag.clone();
                let custom_data = self.parse_custom_tag(tag_clone);
                println!(
                    "{} Skipping top-level custom tag: {:?}",
                    self.dbg(),
                    custom_data
                );
                while self.tokenizer.current_token != Token::Level(0) {
                    self.tokenizer.next_token();
                }
            } else {
                println!(
                    "{} Unhandled token {:?}",
                    self.dbg(),
                    self.tokenizer.current_token
                );
                self.tokenizer.next_token();
            };
        }

        data
    }

    /// Parses HEAD top-level tag. See
    /// https://gedcom.io/specifications/FamilySearchGEDCOMv7.html#HEADER
    fn parse_header(&mut self) -> Header {
        // skip over HEAD tag name
        self.tokenizer.next_token();

        let mut header = Header::default();

        while self.tokenizer.current_token != Token::Level(0) {
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "GEDC" => {
                        header = self.parse_gedcom_data(header);
                    }
                    "SOUR" => header.source = Some(self.parse_head_source()),
                    "DEST" => header.destination = Some(self.take_line_value()),
                    "DATE" => header.date = Some(self.parse_date(1)),
                    "SUBM" => header.submitter_tag = Some(self.take_line_value()),
                    "SUBN" => header.submission_tag = Some(self.take_line_value()),
                    "FILE" => header.filename = Some(self.take_line_value()),
                    "COPR" => header.copyright = Some(self.parse_copyright(1)),
                    "CHAR" => header.encoding = Some(self.parse_encoding_data()),
                    "LANG" => header.language = Some(self.take_line_value()),
                    "NOTE" => header.note = Some(self.parse_note(1)),
                    "PLAC" => header.place = Some(self.parse_head_plac()),
                    _ => panic!("{} Unhandled Header Tag: {}", self.dbg(), tag),
                },
                Token::CustomTag(tag) => {
                    let tag_clone = tag.clone();
                    header.add_custom_data(self.parse_custom_tag(tag_clone))
                }
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Header Token: {:?}",
                    &self.tokenizer.current_token
                ),
            }
        }
        header
    }

    /// parse_head_source handles the SOUR tag in a header
    fn parse_head_source(&mut self) -> HeadSource {
        let mut sour = HeadSource::default();
        sour.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= 1 {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "VERS" => sour.version = Some(self.take_line_value()),
                    "NAME" => sour.name = Some(self.take_line_value()),
                    "CORP" => sour.corporation = Some(self.parse_corporation(2)),
                    "DATA" => sour.data = Some(self.parse_head_data(2)),
                    _ => panic!("{} Unhandled CHAR Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unexpected SOUR Token: {:?}", &self.tokenizer.current_token),
            }
        }
        sour
    }

    /// parse_corporation is for a CORP tag within the SOUR tag of a HEADER
    fn parse_corporation(&mut self, level: u8) -> Corporation {
        let mut corp = Corporation::default();
        corp.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "ADDR" => corp.address = Some(self.parse_address(level + 1)),
                    "PHON" => corp.phone = Some(self.take_line_value()),
                    "EMAIL" => corp.email = Some(self.take_line_value()),
                    "FAX" => corp.fax = Some(self.take_line_value()),
                    "WWW" => corp.website = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled CORP tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled CORP tag in header: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        corp
    }

    /// parse_head_data parses the DATA tag
    fn parse_head_data(&mut self, level: u8) -> HeadSourData {
        let mut data = HeadSourData::default();
        data.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "DATE" => data.date = Some(self.parse_date(level + 1)),
                    "COPR" => data.copyright = Some(self.parse_copyright(level + 1)),
                    _ => panic!("{} unhandled DATA tag in header: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled SOUR tag in header: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        data
    }

    /// parse_head_plac handles the PLAC tag when it is present in header
    fn parse_head_plac(&mut self) -> HeadPlac {
        let mut h_plac = HeadPlac::default();
        // In the header, PLAC should have no payload. See
        // https://gedcom.io/specifications/FamilySearchGEDCOMv7.html#HEAD-PLAC
        self.tokenizer.next_token();
        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= 1 {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "FORM" => {
                        let form = self.take_line_value();
                        let jurisdictional_titles = form.split(",");

                        for t in jurisdictional_titles {
                            let v = t.trim();
                            h_plac.push_jurisdictional_title(v.to_string());
                        }
                    }
                    _ => panic!("{} Unhandled PLAC tag in header: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled PLAC tag in header: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }

        h_plac
    }

    /// parse_copyright handles the COPR tag
    fn parse_copyright(&mut self, level: u8) -> Copyright {
        let mut copyright = Copyright::default();
        copyright.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "CONT" => copyright.continued = Some(self.take_line_value()),
                    "CONC" => copyright.continued = Some(self.take_line_value()),
                    _ => panic!("{} unhandled COPR tag in header: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unhandled tag in COPR: {:?}", self.tokenizer.current_token),
            }
        }
        copyright
    }

    /// Parses SUBM top-level tag
    fn parse_submitter(&mut self, level: u8, xref: Option<String>) -> Submitter {
        // skip over SUBM tag name
        self.tokenizer.next_token();

        let mut submitter = Submitter::new(xref);
        while self.tokenizer.current_token != Token::Level(level) {
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "NAME" => submitter.name = Some(self.take_line_value()),
                    "ADDR" => {
                        submitter.address = Some(self.parse_address(level + 1));
                    }
                    "PHON" => submitter.phone = Some(self.take_line_value()),
                    "LANG" => submitter.language = Some(self.take_line_value()),
                    // "CHAN" => submitter.change_date = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled Submitter Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Submitter Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        // println!("found submitter:\n{:#?}", submitter);
        submitter
    }

    /// Parses INDI top-level tag
    fn parse_individual(&mut self, level: u8, xref: Option<String>) -> Individual {
        // skip over INDI tag name
        self.tokenizer.next_token();
        let mut individual = Individual::new(xref);

        while self.tokenizer.current_token != Token::Level(level) {
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "NAME" => individual.name = Some(self.parse_name(level + 1)),
                    "SEX" => individual.sex = self.parse_gender(),
                    "ADOP" | "BIRT" | "BAPM" | "BARM" | "BASM" | "BLES" | "BURI" | "CENS"
                    | "CHR" | "CHRA" | "CONF" | "CREM" | "DEAT" | "EMIG" | "FCOM" | "GRAD"
                    | "IMMI" | "NATU" | "ORDN" | "RETI" | "RESI" | "PROB" | "WILL" | "EVEN" => {
                        let tag_clone = tag.clone();
                        individual.add_event(self.parse_event(tag_clone.as_str(), level + 1));
                    }
                    "FAMC" | "FAMS" => {
                        let tag_clone = tag.clone();
                        individual
                            .add_family(self.parse_family_link(tag_clone.as_str(), level + 1));
                    }
                    "CHAN" => {
                        // assuming it always only has a single DATE subtag
                        self.tokenizer.next_token(); // level
                        self.tokenizer.next_token(); // DATE tag
                        individual.last_updated = Some(self.take_line_value());
                    }
                    _ => panic!("{} Unhandled Individual Tag: {}", self.dbg(), tag),
                },
                Token::CustomTag(tag) => {
                    let tag_clone = tag.clone();
                    individual.add_custom_data(self.parse_custom_tag(tag_clone))
                }
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Individual Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        // println!("found individual:\n{:#?}", individual);
        individual
    }

    /// Parses FAM top-level tag
    fn parse_family(&mut self, level: u8, xref: Option<String>) -> Family {
        // skip over FAM tag name
        self.tokenizer.next_token();
        let mut family = Family::new(xref);

        while self.tokenizer.current_token != Token::Level(level) {
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "MARR" => family.add_event(self.parse_event("MARR", level + 1)),
                    "HUSB" => family.set_individual1(self.take_line_value()),
                    "WIFE" => family.set_individual2(self.take_line_value()),
                    "CHIL" => family.add_child(self.take_line_value()),
                    _ => panic!("{} Unhandled Family Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unhandled Family Token: {:?}", self.tokenizer.current_token),
            }
        }

        // println!("found family:\n{:#?}", family);
        family
    }

    fn parse_source(&mut self, level: u8, xref: Option<String>) -> Source {
        // skip SOUR tag
        self.tokenizer.next_token();
        let mut source = Source::new(xref);

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "DATA" => self.tokenizer.next_token(),
                    "EVEN" => {
                        let events_recorded = self.take_line_value();
                        let mut event = self.parse_event("OTHER", level + 2);
                        event.with_source_data(events_recorded);
                        source.data.add_event(event);
                    }
                    "AGNC" => source.data.agency = Some(self.take_line_value()),
                    "ABBR" => source.abbreviation = Some(self.take_continued_text(level + 1)),
                    "TITL" => source.title = Some(self.take_continued_text(level + 1)),
                    "REPO" => source.add_repo_citation(self.parse_repo_citation(level + 1)),
                    _ => panic!("{} Unhandled Source Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unhandled Source Token: {:?}", self.tokenizer.current_token),
            }
        }

        // println!("found source:\n{:#?}", source);
        source
    }

    /// Parses REPO top-level tag.
    fn parse_repository(&mut self, level: u8, xref: Option<String>) -> Repository {
        // skip REPO tag
        self.tokenizer.next_token();
        let mut repo = Repository {
            xref,
            name: None,
            address: None,
        };
        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "NAME" => repo.name = Some(self.take_line_value()),
                    "ADDR" => repo.address = Some(self.parse_address(level + 1)),
                    _ => panic!("{} Unhandled Repository Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Repository Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        // println!("found repositiory:\n{:#?}", repo);
        repo
    }

    fn parse_custom_tag(&mut self, tag: String) -> CustomData {
        let value = self.take_line_value();
        CustomData { tag, value }
    }

    /// parse_encoding_data handles the parsing of the CHARS tag
    fn parse_encoding_data(&mut self) -> Encoding {
        let mut encoding = Encoding::default();

        encoding.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= 1 {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "VERS" => encoding.version = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled CHAR Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "{} Unexpected CHAR Token: {:?}",
                    self.dbg(),
                    &self.tokenizer.current_token
                ),
            }
        }
        encoding
    }

    /// parse_data handles the DATE tag
    fn parse_date(&mut self, level: u8) -> Date {
        let mut date = Date::default();
        date.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "TIME" => date.time = Some(self.take_line_value()),
                    _ => panic!("{} unhandled DATE tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unexpected DATE token: {:?}", &self.tokenizer.current_token),
            }
        }
        date
    }

    ///parse_translation handles the TRAN tag
    fn parse_translation(&mut self, level: u8) -> Translation {
        let mut tran = Translation::default();
        tran.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "MIME" => tran.mime = Some(self.take_line_value()),
                    "LANG" => tran.language = Some(self.take_line_value()),
                    _ => panic!("{} unhandled NOTE tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unexpected NOTE token: {:?}", &self.tokenizer.current_token),
            }
        }
        tran
    }

    ///parse_note handles the NOTE tag
    fn parse_note(&mut self, level: u8) -> Note {
        let mut note = Note::default();
        let mut value = String::new();

        value.push_str(&self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "MIME" => note.mime = Some(self.take_line_value()),
                    "TRAN" => note.translation = Some(self.parse_translation(level + 1)),
                    "LANG" => note.language = Some(self.take_line_value()),
                    "CONT" | "CONC" => {
                        value.push('\n');
                        value.push_str(&self.take_line_value());
                    }
                    _ => panic!("{} unhandled NOTE tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unexpected NOTE token: {:?}", &self.tokenizer.current_token),
            }
        }
        if value != "" {
            note.value = Some(value);
        }
        note
    }

    /// Handle parsing GEDC tag
    fn parse_gedcom_data(&mut self, mut header: Header) -> Header {
        let mut gedc = GedcomDocument::default();

        // skip GEDC tag
        self.tokenizer.next_token();

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= 1 {
                    break;
                }
            }

            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "VERS" => gedc.version = Some(self.take_line_value()),
                    // this is the only value that makes sense. warn them otherwise.
                    "FORM" => {
                        let form = self.take_line_value();
                        if &form.to_uppercase() != "LINEAGE-LINKED" {
                            println!(
                                "WARNING: Unrecognized GEDCOM form. Expected LINEAGE-LINKED, found {}"
                            , form);
                        }
                        gedc.form = Some(form);
                    }
                    _ => panic!("{} Unhandled GEDC Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "{} Unexpected GEDC Token: {:?}",
                    self.dbg(),
                    &self.tokenizer.current_token
                ),
            }
        }
        header.gedcom = Some(gedc);
        header
    }

    fn parse_family_link(&mut self, tag: &str, level: u8) -> FamilyLink {
        let xref = self.take_line_value();
        let mut link = FamilyLink::new(xref, tag);

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "PEDI" => link.set_pedigree(self.take_line_value().as_str()),
                    _ => panic!("{} Unhandled FamilyLink Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled FamilyLink Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }

        link
    }

    fn parse_repo_citation(&mut self, level: u8) -> RepoCitation {
        let xref = self.take_line_value();
        let mut citation = RepoCitation {
            xref,
            call_number: None,
        };
        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "CALN" => citation.call_number = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled RepoCitation Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled RepoCitation Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        citation
    }

    fn parse_gender(&mut self) -> Gender {
        self.tokenizer.next_token();
        let gender: Gender;
        if let Token::LineValue(gender_string) = &self.tokenizer.current_token {
            gender = match gender_string.as_str() {
                "M" => Gender::Male,
                "F" => Gender::Female,
                "N" => Gender::Nonbinary,
                "U" => Gender::Unknown,
                _ => panic!("{} Unknown gender value {}", self.dbg(), gender_string),
            };
        } else {
            panic!(
                "Expected gender LineValue, found {:?}",
                self.tokenizer.current_token
            );
        }
        self.tokenizer.next_token();
        gender
    }

    fn parse_name(&mut self, level: u8) -> Name {
        let mut name = Name::default();
        name.value = Some(self.take_line_value());

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "GIVN" => name.given = Some(self.take_line_value()),
                    "NPFX" => name.prefix = Some(self.take_line_value()),
                    "NSFX" => name.suffix = Some(self.take_line_value()),
                    "SPFX" => name.surname_prefix = Some(self.take_line_value()),
                    "SURN" => name.surname = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled Name Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unhandled Name Token: {:?}", self.tokenizer.current_token),
            }
        }

        name
    }

    fn parse_event(&mut self, tag: &str, level: u8) -> Event {
        self.tokenizer.next_token();
        let mut event = Event::from_tag(tag);
        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "DATE" => event.date = Some(self.take_line_value()),
                    "PLAC" => event.place = Some(self.take_line_value()),
                    "SOUR" => event.add_citation(self.parse_citation(level + 1)),
                    _ => panic!("{} Unhandled Event Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!("Unhandled Event Token: {:?}", self.tokenizer.current_token),
            }
        }
        event
    }

    /// Parses ADDR tag
    fn parse_address(&mut self, level: u8) -> Address {
        // skip ADDR tag
        self.tokenizer.next_token();
        let mut address = Address::default();
        let mut value = String::new();

        // handle value on ADDR line
        if let Token::LineValue(addr) = &self.tokenizer.current_token {
            value.push_str(addr);
            self.tokenizer.next_token();
        }

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "CONT" | "CONC" => {
                        value.push('\n');
                        value.push_str(&self.take_line_value());
                    }
                    "ADR1" => address.adr1 = Some(self.take_line_value()),
                    "ADR2" => address.adr2 = Some(self.take_line_value()),
                    "ADR3" => address.adr3 = Some(self.take_line_value()),
                    "CITY" => address.city = Some(self.take_line_value()),
                    "STAE" => address.state = Some(self.take_line_value()),
                    "POST" => address.post = Some(self.take_line_value()),
                    "CTRY" => address.country = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled Address Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Address Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }

        if &value != "" {
            address.value = Some(value);
        }

        address
    }

    fn parse_citation(&mut self, level: u8) -> SourceCitation {
        let mut citation = SourceCitation {
            xref: self.take_line_value(),
            page: None,
        };
        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "PAGE" => citation.page = Some(self.take_line_value()),
                    _ => panic!("{} Unhandled Citation Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Citation Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }
        citation
    }

    /// Takes the value of the current line including handling
    /// multi-line values from CONT & CONC tags.
    fn take_continued_text(&mut self, level: u8) -> String {
        let mut value = self.take_line_value();

        loop {
            if let Token::Level(cur_level) = self.tokenizer.current_token {
                if cur_level <= level {
                    break;
                }
            }
            match &self.tokenizer.current_token {
                Token::Tag(tag) => match tag.as_str() {
                    "CONT" => {
                        value.push('\n');
                        value.push_str(&self.take_line_value())
                    }
                    "CONC" => {
                        value.push(' ');
                        value.push_str(&self.take_line_value())
                    }
                    _ => panic!("{} Unhandled Continuation Tag: {}", self.dbg(), tag),
                },
                Token::Level(_) => self.tokenizer.next_token(),
                _ => panic!(
                    "Unhandled Continuation Token: {:?}",
                    self.tokenizer.current_token
                ),
            }
        }

        value
    }

    /// Grabs and returns to the end of the current line as a String
    fn take_line_value(&mut self) -> String {
        let value: String;
        self.tokenizer.next_token();

        if let Token::LineValue(val) = &self.tokenizer.current_token {
            value = val.to_string();
        } else {
            panic!(
                "{} Expected LineValue, found {:?}",
                self.dbg(),
                self.tokenizer.current_token
            );
        }
        self.tokenizer.next_token();
        value
    }

    /// Debug function displaying GEDCOM line number of error message.
    fn dbg(&self) -> String {
        format!("line {}:", self.tokenizer.line)
    }
}
