// Copyright 2017 The Servo Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use byteorder::{BigEndian, ReadBytesExt};
use euclid::{Point2D, Rect, Size2D};
use otf::FontTable;
use otf::loca::LocaTable;
use std::mem;
use util::Jump;

bitflags! {
    flags Flags: u8 {
        const ON_CURVE = 1 << 0,
        const X_SHORT_VECTOR = 1 << 1,
        const Y_SHORT_VECTOR = 1 << 2,
        const REPEAT = 1 << 3,
        const THIS_X_IS_SAME = 1 << 4,
        const THIS_Y_IS_SAME = 1 << 5,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Point {
    pub position: Point2D<i16>,
    pub on_curve: bool,
    pub first_point_in_contour: bool,
}

/// TODO(pcwalton): Add some caching so we don't keep going to the `loca` table all the time.
#[derive(Clone, Copy, Debug)]
pub struct GlyfTable<'a> {
    pub table: FontTable<'a>,
}

impl<'a> GlyfTable<'a> {
    #[inline]
    pub fn new(table: FontTable) -> GlyfTable {
        GlyfTable {
            table: table,
        }
    }

    pub fn for_each_point<F>(&self, loca_table: &LocaTable, glyph_id: u32, mut callback: F)
                             -> Result<(), ()> where F: FnMut(&Point) {
        let mut reader = self.table.bytes;
        let offset = try!(loca_table.location_of(glyph_id));
        try!(reader.jump(offset as usize));

        let number_of_contours = try!(reader.read_i16::<BigEndian>().map_err(drop));
        if number_of_contours < 0 {
            // TODO(pcwalton): Composite glyphs.
            return Err(())
        }
        try!(reader.jump(mem::size_of::<i16>() * 4));

        // Find out how many points we have.
        let mut endpoints_reader = reader;
        try!(reader.jump(mem::size_of::<u16>() as usize * (number_of_contours as usize - 1)));
        let number_of_points = try!(reader.read_u16::<BigEndian>().map_err(drop)) + 1;

        // Skip over hinting instructions.
        let instruction_length = try!(reader.read_u16::<BigEndian>().map_err(drop));
        try!(reader.jump(instruction_length as usize));

        // Find the offsets of the X and Y coordinates.
        let flags_reader = reader;
        let x_coordinate_length = try!(calculate_size_of_x_coordinates(&mut reader,
                                                                       number_of_points));

        // Set up the streams.
        let mut flag_parser = try!(FlagParser::new(flags_reader));
        let mut x_coordinate_reader = reader;
        try!(reader.jump(x_coordinate_length as usize));
        let mut y_coordinate_reader = reader;

        // Now parse the contours.
        let (mut position, mut point_index) = (Point2D::new(0, 0), 0);
        for _ in 0..number_of_contours {
            let contour_point_count =
                try!(endpoints_reader.read_u16::<BigEndian>().map_err(drop)) - point_index + 1;
            let (mut starting_point, mut last_point_was_off_curve) = (Point2D::new(0, 0), false);
            for contour_point_index in 0..contour_point_count {
                let flags = Flags::from_bits_truncate(*flag_parser.current);
                try!(flag_parser.next());

                let mut delta = Point2D::new(0, 0);
                if flags.contains(X_SHORT_VECTOR) {
                    delta.x = try!(x_coordinate_reader.read_u8().map_err(drop)) as i16;
                    if !flags.contains(THIS_X_IS_SAME) {
                        delta.x = -delta.x
                    }
                } else if !flags.contains(THIS_X_IS_SAME) {
                    delta.x = try!(x_coordinate_reader.read_i16::<BigEndian>().map_err(drop))
                }
                if flags.contains(Y_SHORT_VECTOR) {
                    delta.y = try!(y_coordinate_reader.read_u8().map_err(drop)) as i16;
                    if !flags.contains(THIS_Y_IS_SAME) {
                        delta.y = -delta.y
                    }
                } else if !flags.contains(THIS_Y_IS_SAME) {
                    delta.y = try!(y_coordinate_reader.read_i16::<BigEndian>().map_err(drop))
                }

                if last_point_was_off_curve && !flags.contains(ON_CURVE) {
                    callback(&Point {
                        position: position + delta / 2,
                        on_curve: true,
                        first_point_in_contour: false,
                    })
                }

                position = position + delta;

                let first_point_in_contour = contour_point_index == 0;
                if first_point_in_contour {
                    starting_point = position
                }

                callback(&Point {
                    position: position,
                    on_curve: flags.contains(ON_CURVE),
                    first_point_in_contour: first_point_in_contour,
                });

                last_point_was_off_curve = !flags.contains(ON_CURVE);
                point_index += 1
            }

            // Close the path.
            callback(&Point {
                position: starting_point,
                on_curve: true,
                first_point_in_contour: false,
            })
        }

        Ok(())
    }

    pub fn bounding_rect(&self, loca_table: &LocaTable, glyph_id: u32) -> Result<Rect<i16>, ()> {
        let mut reader = self.table.bytes;
        let offset = try!(loca_table.location_of(glyph_id));
        try!(reader.jump(offset as usize));

        let number_of_contours = try!(reader.read_i16::<BigEndian>().map_err(drop));
        let x_min = try!(reader.read_i16::<BigEndian>().map_err(drop));
        let y_min = try!(reader.read_i16::<BigEndian>().map_err(drop));
        let x_max = try!(reader.read_i16::<BigEndian>().map_err(drop));
        let y_max = try!(reader.read_i16::<BigEndian>().map_err(drop));
        Ok(Rect::new(Point2D::new(x_min, y_min), Size2D::new(x_max - x_min, y_max - y_min)))
    }
}

// Given a reader pointing to the start of the list of flags, returns the size in bytes of the list
// of X coordinates and positions the reader at the start of that list.
#[inline]
fn calculate_size_of_x_coordinates<'a, 'b>(reader: &'a mut &'b [u8], number_of_points: u16)
                                           -> Result<u16, ()> {
    let (mut x_coordinate_length, mut points_left) = (0, number_of_points);
    while points_left > 0 {
        let flags = Flags::from_bits_truncate(try!(reader.read_u8().map_err(drop)));
        let repeat_count = if !flags.contains(REPEAT) {
            1
        } else {
            try!(reader.read_u8().map_err(drop)) as u16 + 1
        };

        if flags.contains(X_SHORT_VECTOR) {
            x_coordinate_length += repeat_count
        } else if !flags.contains(THIS_X_IS_SAME) {
            x_coordinate_length += repeat_count * 2
        }

        points_left -= repeat_count
    }

    Ok(x_coordinate_length)
}

struct FlagParser<'a> {
    next: &'a [u8],
    current: &'a u8,
    repeats_left: u8,
}

impl<'a> FlagParser<'a> {
    #[inline]
    fn new(buffer: &[u8]) -> Result<FlagParser, ()> {
        let mut parser = FlagParser {
            next: buffer,
            current: &buffer[0],
            repeats_left: 0,
        };
        try!(parser.next());
        Ok(parser)
    }

    #[inline]
    fn next(&mut self) -> Result<(), ()> {
        if self.repeats_left > 0 {
            self.repeats_left -= 1;
            return Ok(())
        }

        self.current = try!(self.next.get(0).ok_or(()));
        let flags = Flags::from_bits_truncate(*self.current);
        self.next = &self.next[1..];

        if flags.contains(REPEAT) {
            self.repeats_left = *try!(self.next.get(0).ok_or(()));
            self.next = &self.next[1..];
        } else {
            self.repeats_left = 0
        }

        Ok(())
    }
}

