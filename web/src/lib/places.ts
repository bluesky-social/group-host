// Where a group meets and what it speaks, as a profile holds them (a
// country's ISO 3166 code, a language's BCP 47 tag), named for a reader.

/** Every country by its ISO 3166-1 alpha-2 code. */
const COUNTRY_CODES = (
	'AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ ' +
	'CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR ' +
	'GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP ' +
	'KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS ' +
	'MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS ' +
	'RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW ' +
	'TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW'
).split(' ');

/** The languages offered to choose from, by their BCP 47 tags. */
const LANGUAGE_TAGS = (
	'ar bn ca cs cy da de el en eo es et eu fa fi fil fr ga gl gu he hi hr hu hy id is it ja ka kk ko lt lv mk ml mr ms ' +
	'mt nb nl pa pl pt ro ru sk sl sq sr sv sw ta te th tr uk ur uz vi yo zh zu'
).split(' ');

// Named in English on the server and in the browser alike, so a page
// drawn on one reads the same once the other takes it over.
const regions = new Intl.DisplayNames(['en'], { type: 'region' });
const languages = new Intl.DisplayNames(['en'], { type: 'language' });

const named = (names: Intl.DisplayNames, code: string) => {
	try {
		return names.of(code) ?? code;
	} catch {
		return code;
	}
};

/** A country's name, by its code; the code itself where it names none. */
export const countryName = (code: string) => named(regions, code);

/** A language's name, by its tag; the tag itself where it names none. */
export const languageName = (tag: string) => named(languages, tag);

const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name);

/** Every country, by name. */
export const COUNTRIES = COUNTRY_CODES.map((code) => ({ code, name: countryName(code) })).sort(byName);

/** The languages offered, by name. */
export const LANGUAGES = LANGUAGE_TAGS.map((tag) => ({ tag, name: languageName(tag) })).sort(byName);

/** Where a profile says a group meets, as `community.lexicon.location` has it. */
export type Place = { $type?: string; locality?: string; region?: string; country?: string; name?: string };

export const ADDRESS = 'community.lexicon.location.address';

/**
 * Where a group meets, in words: an address's town and country, or a
 * place's own name; nothing for a place with neither, such as a point on
 * a map.
 */
export function placeName(place?: Place) {
	if (!place) return '';
	if (place.$type === ADDRESS) return [place.locality, place.country && countryName(place.country)].filter(Boolean).join(', ');
	return place.name ?? '';
}
