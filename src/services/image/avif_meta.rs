//! Recovers EXIF and orientation metadata from AVIF containers that
//! `avif-parse` itself discards (#140).
//!
//! `avif-parse` 2.1.0 (the crate `avif_codec.rs` uses for both format
//! detection and dimension peeking - see that file's own doc comment for
//! why it's a direct dependency at all) only exposes the primary item's AV1
//! sequence header. Its public API has no accessor for the `meta` box's
//! item metadata at all - `iinf`/`iloc`/`iprp` are parsed internally (to
//! find the AV1 payload) and then thrown away. So every AVIF decoded
//! through that path silently loses EXIF and orientation, even though both
//! are present in the container: an EXIF blob stashed as a `meta` item
//! (ISO/IEC 23008-12 Annex A.2.1), and orientation expressed as transformative
//! item properties (`irot`/`imir`, ISO/IEC 23008-12 §6.5) associated with
//! the primary item via `iprp`/`ipco`/`ipma` (ISO/IEC 23008-12 §9.3).
//!
//! This module is a small, self-contained, read-only ISOBMFF/HEIF box
//! reader (ISO/IEC 14496-12 §4.2 for the generic `Box` layout; the `meta`
//! box family - `MetaBox`, `ItemInfoBox`/`ItemInfoEntry`,
//! `ItemLocationBox`, `PrimaryItemBox` - is ISO/IEC 14496-12 §8.11) built
//! purpose-fit to recover exactly those two things from the raw file
//! bytes, independent of `avif-parse`. It intentionally does **not**
//! attempt to be a general MP4/HEIF parser.
//!
//! **Out of scope by design: ICC profiles (`colr`).** Nothing in this
//! crate's AVIF encode path (`ravif`/`avif-serialize`, see `avif_codec.rs`)
//! can write a `colr` box back out, so a decoded ICC profile would have
//! nowhere to go on re-encode. That gap is tracked and documented
//! separately from #140.
//!
//! # Safety posture
//!
//! [`parse`] runs on attacker-supplied bytes on every AVIF request. It is
//! infallible by construction: every multi-byte read goes through
//! `slice::get` plus a checked-length conversion (never raw indexing or an
//! `as` truncation that could silently wrap a length), every offset
//! computation uses `checked_add`, and the work is bounded by three named
//! caps ([`MAX_NESTING_DEPTH`], [`MAX_BOXES_VISITED`], [`MAX_EXIF_LEN`]).
//! Anything malformed, truncated, absent, or over a limit degrades to the
//! same "no metadata recovered" result rather than failing the request.

use image::metadata::Orientation;

/// Metadata recovered from an AVIF container.
pub struct AvifMetadata {
    pub orientation: Orientation,
    pub exif: Option<Vec<u8>>,
}

impl Default for AvifMetadata {
    /// `Orientation` has no `Default` impl of its own (it deliberately
    /// doesn't privilege one variant), so this is spelled out by hand. This
    /// is also exactly the "nothing recovered" result every degrade path in
    /// this module falls back to.
    fn default() -> Self {
        Self {
            orientation: Orientation::NoTransforms,
            exif: None,
        }
    }
}

/// Parses `bytes` as an AVIF/HEIF container and recovers what metadata it
/// can. Never errors and never panics: malformed, truncated, or
/// over-the-cap input simply yields fewer (or no) recovered fields. See the
/// module doc comment for the safety posture this relies on.
pub fn parse(bytes: &[u8]) -> AvifMetadata {
    let mut budget = MAX_BOXES_VISITED;

    let Some(meta_content) = find_first_box(bytes, 0, &mut budget, *b"meta") else {
        return AvifMetadata::default();
    };
    // `meta` is itself a FullBox (ISO/IEC 14496-12 §4.2): a 1-byte version
    // and 3-byte flags precede its children.
    let Some(meta_body) = meta_content.get(4..) else {
        return AvifMetadata::default();
    };

    let mut iinf_content: Option<&[u8]> = None;
    let mut iloc_content: Option<&[u8]> = None;
    let mut iprp_content: Option<&[u8]> = None;
    let mut pitm_content: Option<&[u8]> = None;
    walk_boxes(
        meta_body,
        1,
        &mut budget,
        |box_type, content| match &box_type {
            b"iinf" if iinf_content.is_none() => iinf_content = Some(content),
            b"iloc" if iloc_content.is_none() => iloc_content = Some(content),
            b"iprp" if iprp_content.is_none() => iprp_content = Some(content),
            b"pitm" if pitm_content.is_none() => pitm_content = Some(content),
            _ => {}
        },
    );

    let orientation = resolve_orientation(pitm_content, iprp_content, &mut budget)
        .unwrap_or(Orientation::NoTransforms);
    let exif = extract_exif_payload(bytes, iinf_content, iloc_content, &mut budget);

    AvifMetadata { orientation, exif }
}

// ---------------------------------------------------------------------
// Bounds. Every one of these exists to cap work done on attacker-supplied
// bytes; see the module doc comment.
// ---------------------------------------------------------------------

/// Maximum box-container nesting depth this reader will descend into.
/// The deepest legitimate chain this module ever walks is `meta` -> `iprp`
/// -> `ipco` -> a property box (depth 3); 8 leaves generous headroom for
/// oddly-nested-but-valid input while still bounding worst-case work on
/// adversarial input to a small, fixed number of levels.
const MAX_NESTING_DEPTH: u32 = 8;

/// Maximum number of ISOBMFF boxes visited across an entire [`parse`] call
/// (shared across every box-container scan). A well-formed AVIF `meta` box
/// tree has on the order of a dozen boxes; this bounds a buffer packed with
/// thousands of minimal 8-byte boxes to a fixed amount of header-parsing
/// work instead of one proportional to the input size.
const MAX_BOXES_VISITED: u32 = 4096;

/// Maximum size, in bytes, of the EXIF payload this module will recover.
/// ISO/IEC 23008-12 places no limit on the item's size itself; this caps
/// the memory a single AVIF request can make this reader allocate for it.
const MAX_EXIF_LEN: usize = 1024 * 1024;

// ---------------------------------------------------------------------
// Generic box reading
// ---------------------------------------------------------------------

type BoxType = [u8; 4];

/// A checked cursor over a byte slice: every read advances `pos` only on
/// success, and every conversion goes through `slice::get`/`TryFrom` rather
/// than indexing or an `as` cast.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The unread tail of `data`. Always a valid slice: `pos` never exceeds
    /// `data.len()` because every read advances it only after a bounds
    /// check that guarantees it does not.
    fn remainder(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or(&[])
    }

    fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.read_bytes(n).map(|_| ())
    }

    fn read_u8(&mut self) -> Option<u8> {
        let [byte]: [u8; 1] = self.read_bytes(1)?.try_into().ok()?;
        Some(byte)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let bytes = self.read_bytes(2)?;
        Some(u16::from_be_bytes(bytes.try_into().ok()?))
    }

    fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.read_bytes(4)?;
        Some(u32::from_be_bytes(bytes.try_into().ok()?))
    }

    fn read_u64(&mut self) -> Option<u64> {
        let bytes = self.read_bytes(8)?;
        Some(u64::from_be_bytes(bytes.try_into().ok()?))
    }

    /// Reads a big-endian unsigned integer occupying exactly `size` bytes
    /// (the ISOBMFF `ItemLocationBox`'s `offset_size`/`length_size`/
    /// `base_offset_size`/`index_size` fields are each a 4-bit nibble, so
    /// `size` is always in `0..=15` in practice; anything wider than 8
    /// bytes can't fit a `u64` and is rejected rather than wrapped).
    fn read_uint(&mut self, size: u8) -> Option<u64> {
        if size == 0 {
            return Some(0);
        }
        if size > 8 {
            return None;
        }
        let bytes = self.read_bytes(usize::from(size))?;
        Some(bytes.iter().fold(0u64, |acc, &b| (acc << 8) | u64::from(b)))
    }
}

/// Reads one ISOBMFF box header (ISO/IEC 14496-12 §4.2 `Box`) from the
/// start of `data`, honoring both the ordinary 32-bit `size` and the
/// 64-bit `largesize` extension (`size == 1`), and treating `size == 0` as
/// "this box extends to the end of `data`" - valid because every caller
/// passes exactly one container's own content bytes, never a buffer that
/// also holds sibling data past it.
///
/// Returns `(box_type, header_len, total_size)` with `header_len <=
/// total_size <= data.len()` guaranteed, so callers can slice
/// `&data[header_len..total_size]` for the content and `&data[total_size..]`
/// to continue past the box without further bounds checks. Returns `None`
/// for a truncated header or a box whose declared size is smaller than its
/// own header or larger than the remaining buffer.
fn read_box_header(data: &[u8]) -> Option<(BoxType, usize, usize)> {
    let mut cursor = Cursor::new(data);
    let size32 = cursor.read_u32()?;
    let box_type: BoxType = cursor.read_bytes(4)?.try_into().ok()?;
    let (header_len, total_size): (usize, usize) = match size32 {
        1 => {
            let largesize = cursor.read_u64()?;
            let total = usize::try_from(largesize).ok()?;
            (16, total)
        }
        0 => (8, data.len()),
        n => (8, usize::try_from(n).ok()?),
    };
    if total_size < header_len || total_size > data.len() {
        return None;
    }
    Some((box_type, header_len, total_size))
}

/// Invokes `visit(box_type, content)` for every immediate child box in
/// `data`, in order. Stops silently - rather than erroring - on a
/// truncated or oversized box header, once `depth` exceeds
/// [`MAX_NESTING_DEPTH`], or once the shared `budget` is exhausted.
fn walk_boxes<'a>(
    data: &'a [u8],
    depth: u32,
    budget: &mut u32,
    mut visit: impl FnMut(BoxType, &'a [u8]),
) {
    if depth > MAX_NESTING_DEPTH {
        return;
    }
    let mut rest = data;
    while !rest.is_empty() {
        if *budget == 0 {
            return;
        }
        let Some((box_type, header_len, total_size)) = read_box_header(rest) else {
            return;
        };
        *budget -= 1;
        let Some(content) = rest.get(header_len..total_size) else {
            return;
        };
        visit(box_type, content);
        let Some(next) = rest.get(total_size..) else {
            return;
        };
        rest = next;
    }
}

/// The first immediate child of `data` whose type is `target`, or `None` if
/// there isn't one (or it couldn't be reached within the depth/box
/// budget). Every other child is still visited (to keep `budget`
/// accounting simple) but ignored.
fn find_first_box<'a>(
    data: &'a [u8],
    depth: u32,
    budget: &mut u32,
    target: BoxType,
) -> Option<&'a [u8]> {
    let mut found: Option<&'a [u8]> = None;
    walk_boxes(data, depth, budget, |box_type, content| {
        if found.is_none() && box_type == target {
            found = Some(content);
        }
    });
    found
}

// ---------------------------------------------------------------------
// Orientation: iprp/ipco/ipma -> the primary item's irot/imir properties
// ---------------------------------------------------------------------

/// One transformative item property this module understands, together with
/// its raw field (already masked to its defined bit width).
enum TransformProperty {
    /// `irot` (ISO/IEC 23008-12 §6.5.10): counter-clockwise rotation, in
    /// steps of 90 degrees, `0..=3`.
    Rotate(u8),
    /// `imir` (ISO/IEC 23008-12 §6.5.12): mirror axis, `0` (vertical axis,
    /// i.e. a horizontal flip) or `1` (horizontal axis, i.e. a vertical
    /// flip).
    Mirror(u8),
}

/// A single step of the dihedral group of the square (4 rotations x 2
/// reflections - exactly the 8 [`Orientation`] variants), used to compose
/// an ordered sequence of `irot`/`imir` properties into one final
/// orientation.
///
/// Represented as points `z = x + iy` in the complex plane: `Rotation(a)`
/// is the map `z -> z * i^a` (rotate counter-clockwise by `90a` degrees);
/// `Reflection(c)` is the map `z -> conj(z) * i^c`. Every field is always
/// kept in `0..=3` by construction (each arm of [`Self::then`] reduces mod
/// 4), so the handful of small-integer additions below can never overflow
/// a `u8` or underflow it.
#[derive(Clone, Copy)]
enum Transform {
    Rotation(u8),
    Reflection(u8),
}

impl Transform {
    const IDENTITY: Self = Self::Rotation(0);

    /// Composes `self` (applied first) with `op` (applied second),
    /// i.e. returns the transform `z -> op(self(z))`.
    ///
    /// Derived from the complex-number representation above:
    /// rotation-then-rotation adds exponents; rotation-then-reflection and
    /// reflection-then-rotation stay a reflection (exponents combine with a
    /// sign flip for the leading rotation, since reflection conjugates);
    /// reflection-then-reflection composes back to a rotation. This is the
    /// standard dihedral-group multiplication table.
    fn then(self, op: Self) -> Self {
        match (self, op) {
            (Self::Rotation(a), Self::Rotation(b)) => Self::Rotation((a + b) % 4),
            (Self::Rotation(a), Self::Reflection(c)) => Self::Reflection((c + 4 - a % 4) % 4),
            (Self::Reflection(c), Self::Rotation(b)) => Self::Reflection((c + b) % 4),
            (Self::Reflection(c1), Self::Reflection(c2)) => Self::Rotation((c2 + 4 - c1 % 4) % 4),
        }
    }

    /// Maps the final composed transform onto the matching
    /// [`Orientation`] variant, using the same rotate-then-flip-horizontal
    /// convention `image::DynamicImage::apply_orientation` itself uses
    /// (verified against that function's own match arms): `Rotate90` is a
    /// pure clockwise-90 rotation (`z -> z * i^-1`, i.e. `Rotation(3)` in
    /// this counter-clockwise convention), and `Rotate90FlipH` is that
    /// same rotation followed by a horizontal flip.
    fn into_orientation(self) -> Orientation {
        match self {
            Self::Rotation(0) => Orientation::NoTransforms,
            Self::Rotation(1) => Orientation::Rotate270,
            Self::Rotation(2) => Orientation::Rotate180,
            Self::Rotation(3) => Orientation::Rotate90,
            Self::Reflection(0) => Orientation::FlipVertical,
            Self::Reflection(1) => Orientation::Rotate270FlipH,
            Self::Reflection(2) => Orientation::FlipHorizontal,
            Self::Reflection(3) => Orientation::Rotate90FlipH,
            // `then` always reduces exponents mod 4, so every reachable
            // value is covered above; match exhaustively rather than
            // reaching for `unreachable!()` on attacker-influenced data.
            Self::Rotation(_) | Self::Reflection(_) => Orientation::NoTransforms,
        }
    }
}

/// Resolves the primary item's orientation from `iprp`'s content: finds
/// `ipco` (the property list) and `ipma` (the associations), then composes
/// only the `irot`/`imir` properties associated with `primary_item_id`, in
/// the order `ipma` lists them.
fn resolve_orientation(
    pitm_content: Option<&[u8]>,
    iprp_content: Option<&[u8]>,
    budget: &mut u32,
) -> Option<Orientation> {
    let primary_item_id = parse_primary_item_id(pitm_content?)?;
    let iprp_content = iprp_content?;

    let mut ipco_content: Option<&[u8]> = None;
    let mut ipma_content: Option<&[u8]> = None;
    walk_boxes(
        iprp_content,
        2,
        budget,
        |box_type, content| match &box_type {
            b"ipco" if ipco_content.is_none() => ipco_content = Some(content),
            b"ipma" if ipma_content.is_none() => ipma_content = Some(content),
            _ => {}
        },
    );

    let properties = collect_properties(ipco_content?, 3, budget);
    let indices = parse_primary_item_associations(ipma_content?, primary_item_id)?;

    let mut transform = Transform::IDENTITY;
    for index in indices {
        if let Some((_, property)) = properties.iter().find(|(i, _)| *i == index) {
            let op = match property {
                TransformProperty::Rotate(steps) => Transform::Rotation(*steps),
                TransformProperty::Mirror(axis) => {
                    // axis 0 (vertical-axis mirror = horizontal flip) -> reflection exponent 2;
                    // axis 1 (horizontal-axis mirror = vertical flip) -> reflection exponent 0.
                    Transform::Reflection(if *axis == 0 { 2 } else { 0 })
                }
            };
            transform = transform.then(op);
        }
    }
    Some(transform.into_orientation())
}

/// `PrimaryItemBox` ('pitm'): a `FullBox` whose only field is the primary
/// item's ID, 16-bit for version 0 and 32-bit otherwise.
fn parse_primary_item_id(pitm_content: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(pitm_content);
    let version = cursor.read_u8()?;
    cursor.skip(3)?; // flags
    if version == 0 {
        Some(u32::from(cursor.read_u16()?))
    } else {
        cursor.read_u32()
    }
}

/// Walks `ipco`'s children (`ItemPropertyContainerBox`, a plain `Box`
/// containing an implicit, 1-indexed array of property boxes - ISO/IEC
/// 23008-12 §9.3.1). Every child counts toward the index, including ones
/// this module doesn't otherwise understand: skipping the index bump for
/// an uninteresting property type would desynchronize every later index
/// from what `ipma` actually means by it.
fn collect_properties(
    ipco_content: &[u8],
    depth: u32,
    budget: &mut u32,
) -> Vec<(u32, TransformProperty)> {
    if depth > MAX_NESTING_DEPTH {
        return Vec::new();
    }
    let mut properties = Vec::new();
    let mut rest = ipco_content;
    let mut index: u32 = 0;
    while !rest.is_empty() {
        if *budget == 0 {
            break;
        }
        let Some((box_type, header_len, total_size)) = read_box_header(rest) else {
            break;
        };
        *budget -= 1;
        let Some(next_index) = index.checked_add(1) else {
            break;
        };
        index = next_index;
        let Some(content) = rest.get(header_len..total_size) else {
            break;
        };
        match &box_type {
            b"irot" => {
                if let Some(&angle) = content.first() {
                    properties.push((index, TransformProperty::Rotate(angle & 0b11)));
                }
            }
            b"imir" => {
                if let Some(&axis) = content.first() {
                    properties.push((index, TransformProperty::Mirror(axis & 0b1)));
                }
            }
            _ => {}
        }
        let Some(next) = rest.get(total_size..) else {
            break;
        };
        rest = next;
    }
    properties
}

/// `ItemPropertyAssociationBox` ('ipma', ISO/IEC 23008-12 §9.3.1): finds
/// the entry for `target_item_id` and returns its associated property
/// indices, in listed order. Every entry (not only the matching one) must
/// still be parsed in full to keep the cursor aligned with the flat,
/// self-describing-only-by-preceding-fields entry layout.
fn parse_primary_item_associations(ipma_content: &[u8], target_item_id: u32) -> Option<Vec<u32>> {
    let mut cursor = Cursor::new(ipma_content);
    let version = cursor.read_u8()?;
    let flags_bytes = cursor.read_bytes(3)?;
    let flags = flags_bytes
        .iter()
        .fold(0u32, |acc, &b| (acc << 8) | u32::from(b));
    let wide_item_id = version >= 1;
    let large_index = flags & 1 != 0;

    let entry_count = cursor.read_u32()?;
    for _ in 0..entry_count {
        let item_id = if wide_item_id {
            cursor.read_u32()?
        } else {
            u32::from(cursor.read_u16()?)
        };
        let association_count = cursor.read_u8()?;

        if item_id == target_item_id {
            let mut indices = Vec::new();
            for _ in 0..association_count {
                let index = if large_index {
                    u32::from(cursor.read_u16()? & 0x7FFF)
                } else {
                    u32::from(cursor.read_u8()? & 0x7F)
                };
                indices.push(index);
            }
            return Some(indices);
        }

        for _ in 0..association_count {
            if large_index {
                cursor.read_u16()?;
            } else {
                cursor.read_u8()?;
            }
        }
    }
    None
}

// ---------------------------------------------------------------------
// EXIF: iinf/iloc -> the primary EXIF item's bytes
// ---------------------------------------------------------------------

/// Recovers the EXIF item's payload, or `None` if any step along the way
/// is absent, malformed, unsupported, or over [`MAX_EXIF_LEN`].
fn extract_exif_payload(
    bytes: &[u8],
    iinf_content: Option<&[u8]>,
    iloc_content: Option<&[u8]>,
    budget: &mut u32,
) -> Option<Vec<u8>> {
    let exif_item_id = find_exif_item_id(iinf_content?, 2, budget)?;
    let entry = find_iloc_entry(iloc_content?, exif_item_id)?;
    extract_exif_tiff(bytes, &entry)
}

/// Walks `iinf`'s `infe` children (`ItemInfoBox`/`ItemInfoEntry`, ISO/IEC
/// 14496-12 §8.11.6) looking for the one item whose `item_type` is
/// `Exif`, and returns its `item_ID`.
fn find_exif_item_id(iinf_content: &[u8], depth: u32, budget: &mut u32) -> Option<u32> {
    let mut cursor = Cursor::new(iinf_content);
    let version = cursor.read_u8()?;
    cursor.skip(3)?; // flags
    // entry_count itself is unused below (infe boxes are self-delimiting),
    // but must still be read to reach the start of the child box stream.
    if version == 0 {
        cursor.read_u16()?;
    } else {
        cursor.read_u32()?;
    }

    let mut found = None;
    walk_boxes(cursor.remainder(), depth, budget, |box_type, content| {
        if found.is_some() || &box_type != b"infe" {
            return;
        }
        if let Some(item_id) = parse_infe_exif_item_id(content) {
            found = Some(item_id);
        }
    });
    found
}

/// Reads one `infe` entry and returns its `item_ID` if - and only if - it
/// is an `Exif` item. Only the version 2 and 3 layouts are understood
/// (the ones that carry `item_type`); older versions are ignored.
fn parse_infe_exif_item_id(content: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(content);
    let version = cursor.read_u8()?;
    cursor.skip(3)?; // flags

    let item_id = match version {
        2 => u32::from(cursor.read_u16()?),
        3 => cursor.read_u32()?,
        _ => return None,
    };
    cursor.skip(2)?; // item_protection_index
    let item_type: BoxType = cursor.read_bytes(4)?.try_into().ok()?;
    if &item_type == b"Exif" {
        Some(item_id)
    } else {
        None
    }
}

/// One `iloc` entry's location fields, for the item this module cares
/// about.
struct IlocEntry {
    construction_method: u8,
    base_offset: u64,
    extents: Vec<(u64, u64)>,
}

/// `ItemLocationBox` ('iloc', ISO/IEC 14496-12 §8.11.3): finds the entry
/// for `target_item_id`. Every entry must be parsed in full (its field
/// widths are declared once, up front, for the whole box) to keep the
/// cursor aligned, even for items that aren't the one being searched for.
fn find_iloc_entry(iloc_content: &[u8], target_item_id: u32) -> Option<IlocEntry> {
    let mut cursor = Cursor::new(iloc_content);
    let version = cursor.read_u8()?;
    cursor.skip(3)?; // flags

    let sizes = cursor.read_u8()?;
    let offset_size = sizes >> 4;
    let length_size = sizes & 0x0F;
    let sizes2 = cursor.read_u8()?;
    let base_offset_size = sizes2 >> 4;
    let index_size = sizes2 & 0x0F; // only meaningful for version 1/2

    let item_count = if version < 2 {
        u32::from(cursor.read_u16()?)
    } else {
        cursor.read_u32()?
    };

    for _ in 0..item_count {
        let item_id = if version < 2 {
            u32::from(cursor.read_u16()?)
        } else {
            cursor.read_u32()?
        };

        let construction_method = if version == 1 || version == 2 {
            let raw = cursor.read_u16()? & 0x0F;
            u8::try_from(raw).ok()?
        } else {
            0
        };

        cursor.skip(2)?; // data_reference_index
        let base_offset = cursor.read_uint(base_offset_size)?;
        let extent_count = cursor.read_u16()?;

        let mut extents = Vec::new();
        for _ in 0..extent_count {
            if (version == 1 || version == 2) && index_size > 0 {
                cursor.read_uint(index_size)?;
            }
            let extent_offset = cursor.read_uint(offset_size)?;
            let extent_length = cursor.read_uint(length_size)?;
            extents.push((extent_offset, extent_length));
        }

        if item_id == target_item_id {
            return Some(IlocEntry {
                construction_method,
                base_offset,
                extents,
            });
        }
    }
    None
}

/// Assembles `entry`'s extents into the item's raw payload (bounded by
/// [`MAX_EXIF_LEN`]), strips the leading `exif_tiff_header_offset` field
/// (ISO/IEC 23008-12 Annex A.2.1), and returns the TIFF block that
/// follows if it starts with a valid TIFF byte-order marker.
fn extract_exif_tiff(bytes: &[u8], entry: &IlocEntry) -> Option<Vec<u8>> {
    // construction_method 1 (idat) and 2 (item) reference item data
    // elsewhere in the container rather than a plain file offset; this
    // module only supports the common case (0, a file offset), and
    // deliberately does not guess at the others.
    if entry.construction_method != 0 {
        return None;
    }

    let mut payload = Vec::new();
    for &(extent_offset, extent_length) in &entry.extents {
        let start = usize::try_from(entry.base_offset.checked_add(extent_offset)?).ok()?;
        let len = usize::try_from(extent_length).ok()?;
        let end = start.checked_add(len)?;
        let extent = bytes.get(start..end)?;
        if payload.len().checked_add(extent.len())? > MAX_EXIF_LEN {
            return None;
        }
        payload.extend_from_slice(extent);
    }

    // ISO/IEC 23008-12 Annex A.2.1: the item payload begins with a 4-byte
    // big-endian `exif_tiff_header_offset` giving the distance from the
    // end of that field to the start of the TIFF header.
    if payload.len() < 4 {
        return None;
    }
    let (offset_field, tail) = payload.split_at(4);
    let tiff_header_offset = u32::from_be_bytes(offset_field.try_into().ok()?);
    let skip = usize::try_from(tiff_header_offset).ok()?;
    let tiff = tail.get(skip..)?;

    if tiff.starts_with(b"II*\0") || tiff.starts_with(b"MM\0*") {
        Some(tiff.to_vec())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------
    // Fixture helpers: wrap a payload in a standard 32-bit-size box
    // header, or a FullBox header (version + 3-byte flags) on top of
    // that. Keeps the tests below readable as a list of the actual
    // content bytes, rather than hand-counted offsets.
    // -------------------------------------------------------------

    fn boxed(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let size = u32::try_from(payload.len() + 8).expect("test fixture fits in u32");
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(payload);
        out
    }

    fn boxed_size0(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(payload);
        out
    }

    fn full_box(box_type: &[u8; 4], version: u8, flags: [u8; 3], payload: &[u8]) -> Vec<u8> {
        let mut body = vec![version];
        body.extend_from_slice(&flags);
        body.extend_from_slice(payload);
        boxed(box_type, &body)
    }

    fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.iter().flat_map(|p| p.iter().copied()).collect()
    }

    /// Strips the standard 8-byte (size + type) box header a `boxed`/
    /// `full_box` fixture starts with, yielding just its content - what
    /// `resolve_orientation` etc. expect to be handed directly (matching
    /// what `walk_boxes` would have already stripped on the real
    /// `parse()` path).
    fn box_content(b: &[u8]) -> &[u8] {
        &b[8..]
    }

    /// One property, one association, one item: a minimal `iprp` whose
    /// primary item has exactly the given `irot`/`imir` properties applied
    /// in the given order. `props` is a list of (box_type, payload_byte).
    fn iprp_with_properties(primary_item_id: u16, props: &[(&[u8; 4], u8)]) -> Vec<u8> {
        let mut ipco_payload = Vec::new();
        let mut associations = Vec::new();
        for (index, (box_type, payload_byte)) in props.iter().enumerate() {
            ipco_payload.extend_from_slice(&boxed(box_type, &[*payload_byte]));
            // 1-based property index, non-essential (top bit 0), narrow (7-bit) form.
            let property_index = u8::try_from(index + 1).expect("test has few properties");
            associations.push(property_index & 0x7F);
        }
        let ipco = boxed(b"ipco", &ipco_payload);

        let mut ipma_payload = 1u32.to_be_bytes().to_vec(); // entry_count
        ipma_payload.extend_from_slice(&primary_item_id.to_be_bytes());
        ipma_payload.push(u8::try_from(associations.len()).expect("test has few associations"));
        ipma_payload.extend_from_slice(&associations);
        let ipma = full_box(b"ipma", 0, [0, 0, 0], &ipma_payload);

        boxed(b"iprp", &concat(&[ipco, ipma]))
    }

    fn pitm(item_id: u16) -> Vec<u8> {
        full_box(b"pitm", 0, [0, 0, 0], &item_id.to_be_bytes())
    }

    fn meta(children: &[Vec<u8>]) -> Vec<u8> {
        full_box(b"meta", 0, [0, 0, 0], &concat(children))
    }

    /// A well-formed `infe`/`iinf` pair declaring `item_id` as an `Exif`
    /// item (version 2 layout).
    fn iinf_with_exif_item(item_id: u16) -> Vec<u8> {
        let mut infe_payload = item_id.to_be_bytes().to_vec();
        infe_payload.extend_from_slice(&0u16.to_be_bytes()); // item_protection_index
        infe_payload.extend_from_slice(b"Exif");
        let infe = full_box(b"infe", 2, [0, 0, 0], &infe_payload);

        let mut iinf_payload = 1u16.to_be_bytes().to_vec(); // entry_count
        iinf_payload.extend_from_slice(&infe);
        full_box(b"iinf", 0, [0, 0, 0], &iinf_payload)
    }

    /// An `iloc` (version 0, construction_method implicitly 0) with a
    /// single item/single extent pointing at `(offset, length)` in the
    /// eventual file buffer.
    fn iloc_single_extent(item_id: u16, offset: u32, length: u32) -> Vec<u8> {
        let mut payload = vec![0x44, 0x00]; // offset_size=4, length_size=4; base_offset_size=0, index_size=0
        payload.extend_from_slice(&1u16.to_be_bytes()); // item_count
        payload.extend_from_slice(&item_id.to_be_bytes()); // item_ID
        payload.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
        // base_offset: width 0, omitted entirely.
        payload.extend_from_slice(&1u16.to_be_bytes()); // extent_count
        payload.extend_from_slice(&offset.to_be_bytes());
        payload.extend_from_slice(&length.to_be_bytes());
        full_box(b"iloc", 0, [0, 0, 0], &payload)
    }

    /// A raw EXIF item payload: the 4-byte `exif_tiff_header_offset`
    /// (always 0 here) followed by a minimal TIFF block.
    fn exif_item_bytes(magic: &[u8; 4]) -> Vec<u8> {
        let mut out = 0u32.to_be_bytes().to_vec();
        out.extend_from_slice(magic);
        out.extend_from_slice(&[0, 0, 0, 0]); // stand-in IFD offset field
        out
    }

    const FIXTURE_PRIMARY_ITEM_ID: u16 = 1;
    const FIXTURE_EXIF_ITEM_ID: u16 = 2;

    /// Builds the `meta` box for [`full_fixture`], given the byte offset
    /// (within the eventual file) of the EXIF item's raw bytes that follow
    /// it. `iloc`'s field widths never depend on the offset/length
    /// *values* it stores, only on `props` (which change `iprp`'s size) -
    /// so building this twice (once with a placeholder offset purely to
    /// measure its length, once for real) always yields the same length
    /// both times, sidestepping the chicken-and-egg problem of the offset
    /// depending on this box's own total size.
    fn fixture_meta_box(props: &[(&[u8; 4], u8)], exif_offset: u32, exif_len: u32) -> Vec<u8> {
        meta(&[
            pitm(FIXTURE_PRIMARY_ITEM_ID),
            iprp_with_properties(FIXTURE_PRIMARY_ITEM_ID, props),
            iinf_with_exif_item(FIXTURE_EXIF_ITEM_ID),
            iloc_single_extent(FIXTURE_EXIF_ITEM_ID, exif_offset, exif_len),
        ])
    }

    /// Assembles a complete, well-formed AVIF-style file: a `meta` box
    /// (with `pitm`, `iprp` with the given properties, `iinf`, `iloc`)
    /// followed by the EXIF item's raw bytes it points at.
    fn full_fixture(props: &[(&[u8; 4], u8)], exif_magic: &[u8; 4]) -> Vec<u8> {
        let exif_bytes = exif_item_bytes(exif_magic);
        let exif_len = u32::try_from(exif_bytes.len()).unwrap();
        let exif_offset = u32::try_from(fixture_meta_box(props, 0, exif_len).len()).unwrap();

        let mut file = fixture_meta_box(props, exif_offset, exif_len);
        file.extend_from_slice(&exif_bytes);
        file
    }

    // -------------------------------------------------------------
    // Box-structure robustness
    // -------------------------------------------------------------

    #[test]
    fn no_meta_box_yields_default() {
        let file = boxed(b"ftyp", b"avifavif");
        let result = parse(&file);
        assert_eq!(result.orientation, Orientation::NoTransforms);
        assert!(result.exif.is_none());
    }

    #[test]
    fn truncated_box_header_does_not_panic() {
        for len in 0..8 {
            let file = vec![0xAAu8; len];
            let result = parse(&file);
            assert_eq!(result.orientation, Orientation::NoTransforms);
            assert!(result.exif.is_none());
        }
    }

    #[test]
    fn oversized_declared_box_size_is_rejected() {
        let mut file = Vec::new();
        file.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes()); // declared size, absurd
        file.extend_from_slice(b"meta");
        file.extend_from_slice(&[0u8; 4]); // a few real bytes, nowhere near declared size
        let result = parse(&file);
        assert_eq!(result.orientation, Orientation::NoTransforms);
        assert!(result.exif.is_none());
    }

    #[test]
    fn size_zero_extends_to_end_of_buffer() {
        // A meta box using size == 0 as the last (only) box in the file
        // must still be found and parsed like any other.
        let payload = {
            let mut body = vec![0u8, 0, 0, 0]; // meta's own version+flags
            body.extend_from_slice(&pitm(1));
            body.extend_from_slice(&iprp_with_properties(1, &[(b"irot", 1)]));
            body
        };
        let file = boxed_size0(b"meta", &payload);
        let result = parse(&file);
        assert_eq!(result.orientation, Orientation::Rotate270);
    }

    #[test]
    fn deep_nesting_past_the_cap_visits_nothing() {
        let leaf = boxed(b"irot", &[1]);
        let mut budget = MAX_BOXES_VISITED;
        let mut visited = 0;
        walk_boxes(&leaf, MAX_NESTING_DEPTH + 1, &mut budget, |_, _| {
            visited += 1
        });
        assert_eq!(visited, 0);

        let mut budget = MAX_BOXES_VISITED;
        let properties = collect_properties(&leaf, MAX_NESTING_DEPTH + 1, &mut budget);
        assert!(properties.is_empty());
    }

    // -------------------------------------------------------------
    // Mutation fuzzing: no fixture, however corrupted, may panic.
    // -------------------------------------------------------------

    #[test]
    fn mutations_of_a_valid_fixture_never_panic() {
        let fixture = full_fixture(&[(b"irot", 1), (b"imir", 0)], b"II*\0");

        // Fixed-seed linear congruential generator: deterministic without
        // pulling in a `rand`-style dependency just for a test.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state
        };

        for _ in 0..500 {
            let mut mutated = fixture.clone();
            if mutated.is_empty() {
                continue;
            }
            let index = usize::try_from(next() % u64::try_from(mutated.len()).unwrap()).unwrap();
            let flip = u8::try_from(next() % 256).unwrap();
            mutated[index] ^= flip;
            let _ = parse(&mutated); // must not panic
        }
    }

    // -------------------------------------------------------------
    // Orientation composition
    // -------------------------------------------------------------

    #[test]
    fn irot_alone_maps_every_angle() {
        let mut budget = MAX_BOXES_VISITED;
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"irot", 0)]))),
                &mut budget,
            ),
            Some(Orientation::NoTransforms)
        );
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"irot", 1)]))),
                &mut budget,
            ),
            Some(Orientation::Rotate270)
        );
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"irot", 2)]))),
                &mut budget,
            ),
            Some(Orientation::Rotate180)
        );
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"irot", 3)]))),
                &mut budget,
            ),
            Some(Orientation::Rotate90)
        );
    }

    #[test]
    fn imir_alone_maps_each_axis() {
        let mut budget = MAX_BOXES_VISITED;
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"imir", 0)]))),
                &mut budget,
            ),
            Some(Orientation::FlipHorizontal)
        );
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(1, &[(b"imir", 1)]))),
                &mut budget,
            ),
            Some(Orientation::FlipVertical)
        );
    }

    #[test]
    fn irot_then_imir_and_imir_then_irot_differ() {
        let mut budget = MAX_BOXES_VISITED;
        // irot(CCW 90) then imir(axis 0 / horizontal flip):
        // Rotation(1).then(Reflection(2)) = Reflection((2+4-1)%4) = Reflection(1) -> Rotate270FlipH.
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(
                    1,
                    &[(b"irot", 1), (b"imir", 0)],
                ))),
                &mut budget,
            ),
            Some(Orientation::Rotate270FlipH)
        );
        // imir(axis 0) then irot(CCW 90):
        // Reflection(2).then(Rotation(1)) = Reflection((2+1)%4) = Reflection(3) -> Rotate90FlipH.
        assert_eq!(
            resolve_orientation(
                Some(box_content(&pitm(1))),
                Some(box_content(&iprp_with_properties(
                    1,
                    &[(b"imir", 0), (b"irot", 1)],
                ))),
                &mut budget,
            ),
            Some(Orientation::Rotate90FlipH)
        );
    }

    #[test]
    fn properties_of_a_non_primary_item_are_ignored() {
        // ipma associates properties with item 99; the primary item is 1
        // and has an entry of its own with zero associations.
        let ipco = boxed(b"ipco", &boxed(b"irot", &[1]));
        let mut ipma_payload = 2u32.to_be_bytes().to_vec(); // entry_count
        ipma_payload.extend_from_slice(&1u16.to_be_bytes()); // item_ID (primary)
        ipma_payload.push(0); // association_count: none
        ipma_payload.extend_from_slice(&99u16.to_be_bytes()); // item_ID (not primary)
        ipma_payload.push(1); // association_count
        ipma_payload.push(1); // property index 1 (irot), non-essential
        let ipma = full_box(b"ipma", 0, [0, 0, 0], &ipma_payload);
        let iprp_content = concat(&[ipco, ipma]);
        let mut budget = MAX_BOXES_VISITED;

        let orientation = resolve_orientation(
            Some(box_content(&pitm(1))),
            Some(&iprp_content),
            &mut budget,
        );
        assert_eq!(orientation, Some(Orientation::NoTransforms));
    }

    // -------------------------------------------------------------
    // EXIF extraction
    // -------------------------------------------------------------

    #[test]
    fn well_formed_exif_item_round_trips() {
        let file = full_fixture(&[], b"II*\0");
        let result = parse(&file);
        let exif = result.exif.expect("exif recovered");
        assert!(exif.starts_with(b"II*\0"));
        assert_eq!(exif.len(), 8); // magic (4) + stand-in IFD offset field (4)
    }

    #[test]
    fn wrong_tiff_magic_yields_none() {
        let file = full_fixture(&[], b"XXXX");
        let result = parse(&file);
        assert!(result.exif.is_none());
    }

    #[test]
    fn exif_item_larger_than_cap_yields_none() {
        let oversized = vec![b'x'; MAX_EXIF_LEN + 1];
        let entry = IlocEntry {
            construction_method: 0,
            base_offset: 0,
            extents: vec![(0, u64::try_from(oversized.len()).unwrap())],
        };
        assert!(extract_exif_tiff(&oversized, &entry).is_none());
    }

    #[test]
    fn construction_method_idat_yields_none() {
        let bytes = exif_item_bytes(b"II*\0");
        let entry = IlocEntry {
            construction_method: 1, // idat: unsupported by design
            base_offset: 0,
            extents: vec![(0, u64::try_from(bytes.len()).unwrap())],
        };
        assert!(extract_exif_tiff(&bytes, &entry).is_none());
    }
}
