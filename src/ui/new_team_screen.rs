use super::button::Button;
use super::clickable_list::ClickableListState;
use super::constants::{UiText, LEFT_PANEL_WIDTH};
use super::gif_map::GifMap;
use super::traits::SplitPanel;

use super::ui_callback::{CallbackRegistry, UiCallbackPreset};
use super::{
    constants::UiStyle,
    traits::Screen,
    utils::{img_to_lines, input_from_key_event},
    widgets::{default_block, render_player_description, selectable_list},
};
use crate::image::color_map::ColorPreset;
use crate::image::spaceship::SPACESHIP_IMAGE_WIDTH;
use crate::types::{AppResult, AU, HOURS};
use crate::world::constants::{CURRENCY_SYMBOL, INITIAL_TEAM_BALANCE};
use crate::world::spaceship::{Spaceship, SpaceshipPrefab};
use crate::{
    image::color_map::ColorMap,
    types::{PlanetId, PlayerId},
    world::{
        jersey::{Jersey, JerseyStyle},
        skill::Rated,
        world::World,
    },
};
use core::fmt::Debug;
use core::panic;
use crossterm::event::{KeyCode, KeyEvent};
use itertools::Itertools;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::style::Styled;
use ratatui::text::Line;
use ratatui::{
    prelude::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Clear, Paragraph, Wrap},
    Frame,
};
use std::cmp::min;
use std::collections::HashMap;
use std::{cell::RefCell, rc::Rc};
use strum::IntoEnumIterator;
use tui_textarea::{CursorMove, TextArea};

const INITIAL_TEAM_SIZE: usize = 6;
const MIN_NAME_LENGTH: usize = 3;
const MAX_NAME_LENGTH: usize = 12;

#[derive(Debug, Default, PartialOrd, PartialEq)]
pub enum CreationState {
    #[default]
    TeamName,
    ShipName,
    Planet,
    Jersey,
    ShipModel,
    Players,
    Done,
}

impl CreationState {
    pub fn next(&self) -> Self {
        match self {
            CreationState::TeamName => CreationState::ShipName,
            CreationState::ShipName => CreationState::Planet,
            CreationState::Planet => CreationState::Jersey,
            CreationState::Jersey => CreationState::ShipModel,
            CreationState::ShipModel => CreationState::Players,
            CreationState::Players => CreationState::Done,
            CreationState::Done => CreationState::Done,
        }
    }

    pub fn previous(&self) -> Self {
        match self {
            CreationState::TeamName => CreationState::TeamName,
            CreationState::ShipName => CreationState::TeamName,
            CreationState::Planet => CreationState::ShipName,
            CreationState::Jersey => CreationState::Planet,
            CreationState::ShipModel => CreationState::Jersey,
            CreationState::Players => CreationState::ShipModel,
            CreationState::Done => CreationState::Players,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
enum ConfirmChoice {
    #[default]
    Yes,
    No,
}

#[derive(Debug, Default)]
pub struct NewTeamScreen {
    state: CreationState,
    tick: usize,
    team_name_textarea: TextArea<'static>,
    ship_name_textarea: TextArea<'static>,
    spaceship_models: Vec<SpaceshipPrefab>,
    spaceship_model_index: usize,
    planet_index: usize,
    planet_ids: Vec<PlanetId>,
    jersey_styles: Vec<JerseyStyle>,
    jersey_style_index: usize,
    red_color_preset: ColorPreset,
    green_color_preset: ColorPreset,
    blue_color_preset: ColorPreset,
    player_index: usize,
    // Map of planet_id -> (player_id, hiring cost)
    planet_players: HashMap<PlanetId, Vec<(PlayerId, u32)>>,
    selected_players: Vec<PlayerId>,
    confirm: ConfirmChoice,
    callback_registry: Rc<RefCell<CallbackRegistry>>,
    gif_map: Rc<RefCell<GifMap>>,
}

impl NewTeamScreen {
    pub fn new(
        callback_registry: Rc<RefCell<CallbackRegistry>>,
        gif_map: Rc<RefCell<GifMap>>,
    ) -> Self {
        let mut team_name_textarea = TextArea::default();
        team_name_textarea.set_cursor_style(UiStyle::SELECTED);
        team_name_textarea.set_block(
            default_block()
                .border_style(UiStyle::DEFAULT)
                .title("Team name"),
        );
        let mut ship_name_textarea = TextArea::default();
        ship_name_textarea.set_cursor_style(UiStyle::DEFAULT);
        ship_name_textarea.set_block(
            default_block()
                .border_style(UiStyle::UNSELECTABLE)
                .title("Ship name"),
        );
        let rng = &mut ChaCha8Rng::from_entropy();
        let mut color_presets = ColorPreset::iter().collect::<Vec<_>>();
        color_presets.shuffle(rng);
        let red_color_preset = color_presets[0];
        let green_color_preset = color_presets[1];
        let blue_color_preset = color_presets[2];

        let jersey_styles = JerseyStyle::iter()
            .filter(|jersey_style| jersey_style.is_available_at_creation())
            .collect_vec();
        let spaceship_models = SpaceshipPrefab::iter().collect_vec();
        Self {
            team_name_textarea,
            ship_name_textarea,
            red_color_preset,
            green_color_preset,
            blue_color_preset,
            spaceship_models,
            jersey_styles,
            callback_registry,
            gif_map,
            ..Default::default()
        }
    }

    fn selected_ship(&self) -> Spaceship {
        let prefab = self.spaceship_models[self.spaceship_model_index];
        let name = self.ship_name_textarea.lines()[0].clone();
        let color_map = self.get_team_colors();
        prefab.specs(name, color_map)
    }

    fn get_team_colors(&self) -> ColorMap {
        ColorMap {
            red: self.red_color_preset.to_rgb(),
            green: self.green_color_preset.to_rgb(),
            blue: self.blue_color_preset.to_rgb(),
        }
    }

    pub fn set_team_colors(&mut self, color: ColorPreset, channel: usize) {
        match channel {
            0 => self.red_color_preset = color,
            1 => self.green_color_preset = color,
            2 => self.blue_color_preset = color,
            _ => panic!("Invalid color index"),
        }
    }

    pub fn clear_selected_players(&mut self) {
        self.selected_players.clear();
    }

    pub fn set_state(&mut self, state: CreationState) {
        self.state = state;
        self.set_index(0);
    }

    fn render_intro(&mut self, frame: &mut Frame, area: Rect) {
        let text = format!(
            "
        It's the year 2101. Corporations have taken over the world. 
        The only way to be free is to join a pirate crew and start plundering the galaxy.
        The only mean of survival is to play basketball.

        Now it's your turn to go out there and make a name for yourself.
        Create your crew and start wondering the galaxy in search of worthy basketball opponents.
        
        Choose your team name, customize your ship, and select a worthy crew.

        [Press enter to confirm selections.]"
        );

        let paragraph = Paragraph::new(text);
        frame.render_widget(
            paragraph
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center),
            area,
        );

        // Render main block
        frame.render_widget(default_block(), area);
    }

    fn render_spaceship(&mut self, frame: &mut Frame, area: Rect) {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SPACESHIP_IMAGE_WIDTH as u16 + 2),
                Constraint::Min(1),
            ])
            .split(area.inner(&Margin {
                horizontal: 1,
                vertical: 1,
            }));

        if let Ok(gif) = self.selected_ship().compose_image() {
            let img = gif[(self.tick) % gif.len()].clone();
            let paragraph = Paragraph::new(img_to_lines(&img));
            frame.render_widget(
                paragraph.alignment(Alignment::Center),
                split[0].inner(&Margin {
                    vertical: 0,
                    horizontal: 1,
                }),
            );
        }
        let spaceship = self.selected_ship();
        let spaceship_info = Paragraph::new(vec![
            Line::from(format!("Ship name: {}", spaceship.name.to_string())),
            Line::from(format!(
                "Speed: {:.3} AU/h",
                spaceship.speed() * HOURS as f32 / AU as f32
            )),
            Line::from(format!("Capacity: {}", spaceship.capacity())),
            Line::from(format!(
                "Consumption: {:.2} t/h",
                spaceship.fuel_consumption() * HOURS as f32
            )),
            Line::from(format!("Tank: {} t", spaceship.tank())),
            Line::from(format!(
                "Max distance: {:.0} AU",
                spaceship.max_distance() / AU as f32
            )),
            // Line::from(format!(
            //     "Autonomy: {}",
            //     spaceship.max_travel_time().formatted()
            // )),
            Line::from(format!("Cost: {} {}", spaceship.cost(), CURRENCY_SYMBOL)),
        ]);

        frame.render_widget(
            spaceship_info,
            split[1].inner(&Margin {
                vertical: 1,
                horizontal: 1,
            }),
        );

        // Render main block
        frame.render_widget(default_block(), area);
    }

    fn render_spaceship_selection(&self, frame: &mut Frame, area: Rect) {
        if self.state > CreationState::ShipModel {
            let selected_ship = self.spaceship_models[self.spaceship_model_index];
            frame.render_widget(
                Paragraph::new(format!(" {}", selected_ship)).block(
                    default_block()
                        .border_style(UiStyle::OK)
                        .title("Choose spaceship model ↓/↑"),
                ),
                area,
            );
        } else if self.state == CreationState::ShipModel {
            let options = SpaceshipPrefab::iter()
                .map(|ship| {
                    (
                        format!("{:<12} {:>6} {}", ship, ship.cost(), CURRENCY_SYMBOL),
                        UiStyle::DEFAULT,
                    )
                })
                .collect_vec();

            let list = selectable_list(options, &self.callback_registry);
            frame.render_stateful_widget(
                list.block(
                    default_block()
                        .border_style(UiStyle::DEFAULT)
                        .title("Choose spaceship model ↓/↑"),
                ),
                area,
                &mut ClickableListState::default().with_selected(Some(self.spaceship_model_index)),
            );
        } else {
            frame.render_widget(
                default_block()
                    .border_style(UiStyle::UNSELECTABLE)
                    .title("Choose spaceship model ↓/↑"),
                area,
            );
        }
    }

    fn render_jersey_selection(&mut self, frame: &mut Frame, area: Rect) {
        if self.state > CreationState::Jersey {
            let selected_jersey_style = self.jersey_styles[self.jersey_style_index];
            frame.render_widget(
                Paragraph::new(format!(" {}", selected_jersey_style)).block(
                    default_block()
                        .border_style(UiStyle::OK)
                        .title("Choose jersey style ↓/↑"),
                ),
                area,
            );
        } else if self.state == CreationState::Jersey {
            let options = self
                .jersey_styles
                .iter()
                .map(|jersey_style| (format!("{}", jersey_style), UiStyle::DEFAULT))
                .collect_vec();

            let list = selectable_list(options, &self.callback_registry);
            frame.render_stateful_widget(
                list.block(
                    default_block()
                        .border_style(UiStyle::DEFAULT)
                        .title("Choose jersey style ↓/↑"),
                ),
                area,
                &mut ClickableListState::default().with_selected(Some(self.jersey_style_index)),
            );
        } else {
            frame.render_widget(
                default_block()
                    .border_style(UiStyle::UNSELECTABLE)
                    .title("Choose jersey style ↓/↑"),
                area,
            );
        }
    }

    fn render_jersey(&self, frame: &mut Frame, world: &World, area: Rect) {
        let style = self.jersey_styles[self.jersey_style_index];
        let planet_id = self.planet_ids[self.planet_index];
        let planet_players = &self
            .planet_players
            .get(&planet_id)
            .unwrap_or_else(|| panic!("No players found for planet {}", planet_id.to_string()));
        let mut player = world.get_player(planet_players[0].0).unwrap().clone();
        let jersey = Jersey {
            style,
            color: self.get_team_colors(),
        };
        player.image.set_jersey(&jersey, &player.info);

        // We cannot use the gif map because we are changing the jersey style
        if let Ok(gif) = player.compose_image() {
            let img = gif[(self.tick / 8) % gif.len()].clone();
            let paragraph = Paragraph::new(img_to_lines(&img));
            frame.render_widget(
                paragraph.alignment(Alignment::Center),
                area.inner(&Margin {
                    vertical: 1,
                    horizontal: 1,
                }),
            );
        }

        // Render main block
        frame.render_widget(default_block(), area);
    }

    fn render_colors_selection(&self, frame: &mut Frame, area: Rect) {
        let border_style = if self.state > CreationState::ShipModel {
            UiStyle::OK
        } else if self.state >= CreationState::Jersey {
            UiStyle::DEFAULT
        } else {
            UiStyle::UNSELECTABLE
        };

        let color_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(area);

        if self.state >= CreationState::Jersey {
            let red_style = Style::default().bg(Color::Rgb(
                self.get_team_colors().red[0],
                self.get_team_colors().red[1],
                self.get_team_colors().red[2],
            ));
            let red = Button::text(
                vec![
                    Line::from(Span::styled(" ".repeat(area.width as usize / 3), red_style)),
                    Line::from(Span::styled(" ".repeat(area.width as usize / 3), red_style)),
                ]
                .into(),
                UiCallbackPreset::SetTeamColors {
                    color: self.red_color_preset.next(),
                    channel: 0,
                },
                Rc::clone(&self.callback_registry),
            );
            let green_style = Style::default().bg(Color::Rgb(
                self.get_team_colors().green[0],
                self.get_team_colors().green[1],
                self.get_team_colors().green[2],
            ));
            let green = Button::text(
                vec![
                    Line::from(Span::styled(
                        " ".repeat(area.width as usize / 3),
                        green_style,
                    )),
                    Line::from(Span::styled(
                        " ".repeat(area.width as usize / 3),
                        green_style,
                    )),
                ]
                .into(),
                UiCallbackPreset::SetTeamColors {
                    color: self.green_color_preset.next(),
                    channel: 1,
                },
                Rc::clone(&self.callback_registry),
            );

            let blue_style = Style::default().bg(Color::Rgb(
                self.get_team_colors().blue[0],
                self.get_team_colors().blue[1],
                self.get_team_colors().blue[2],
            ));
            let blue = Button::text(
                vec![
                    Line::from(Span::styled(
                        " ".repeat(area.width as usize / 3),
                        blue_style,
                    )),
                    Line::from(Span::styled(
                        " ".repeat(area.width as usize / 3),
                        blue_style,
                    )),
                ]
                .into(),
                UiCallbackPreset::SetTeamColors {
                    color: self.blue_color_preset.next(),
                    channel: 2,
                },
                Rc::clone(&self.callback_registry),
            );

            frame.render_widget(
                red,
                color_split[0].inner(&Margin {
                    horizontal: 1,
                    vertical: 1,
                }),
            );
            frame.render_widget(
                green,
                color_split[1].inner(&Margin {
                    horizontal: 1,
                    vertical: 1,
                }),
            );
            frame.render_widget(
                blue,
                color_split[2].inner(&Margin {
                    horizontal: 1,
                    vertical: 1,
                }),
            );
        }

        frame.render_widget(
            default_block()
                .border_style(border_style)
                .title("Choose 'r'"),
            color_split[0],
        );
        frame.render_widget(
            default_block()
                .border_style(border_style)
                .title("Choose 'g'"),
            color_split[1],
        );
        frame.render_widget(
            default_block()
                .border_style(border_style)
                .title("Choose 'b'"),
            color_split[2],
        );
    }

    fn render_planet_selection(&mut self, frame: &mut Frame, world: &World, area: Rect) {
        if self.state > CreationState::Planet {
            let selected_planet = world
                .get_planet_or_err(self.planet_ids[self.planet_index])
                .unwrap();
            frame.render_widget(
                Paragraph::new(format!(" {}", selected_planet.name.clone())).block(
                    default_block()
                        .border_style(UiStyle::OK)
                        .title("Choose planet ↓/↑"),
                ),
                area,
            );
        } else if self.state == CreationState::Planet {
            let options = self
                .planet_ids
                .iter()
                .map(|&planet_id| {
                    let planet = world.get_planet_or_err(planet_id).unwrap();
                    (planet.name.clone(), UiStyle::DEFAULT)
                })
                .collect_vec();

            let list = selectable_list(options, &self.callback_registry);
            frame.render_stateful_widget(
                list.block(
                    default_block()
                        .border_style(UiStyle::DEFAULT)
                        .title("Choose planet ↓/↑"),
                ),
                area,
                &mut ClickableListState::default().with_selected(Some(self.planet_index)),
            );
        } else {
            frame.render_widget(
                default_block()
                    .border_style(UiStyle::UNSELECTABLE)
                    .title("Choose planet ↓/↑"),
                area,
            );
        }
    }

    fn render_planet(&mut self, frame: &mut Frame, world: &World, area: Rect) {
        let planet_id = self.planet_ids[self.planet_index];
        let planet = world.get_planet_or_err(planet_id).unwrap();

        let frame_lines = self.gif_map.borrow_mut().planet_zoom_in_frame_lines(
            planet_id,
            self.tick / planet.rotation_period,
            world,
        );
        if frame_lines.is_err() {
            return;
        }
        let mut lines = frame_lines.unwrap();

        // Apply y-centering
        let min_offset = if lines.len() > area.height as usize {
            (lines.len() - area.height as usize) / 2
        } else {
            0
        };
        let max_offset = min(lines.len(), min_offset + area.height as usize);
        lines = lines[min_offset..max_offset].to_vec();

        // Apply x-centering
        if lines[0].spans.len() > area.width as usize - 2 {
            let min_offset = if lines[0].spans.len() > area.width as usize {
                (lines[0].spans.len() - area.width as usize) / 2
            } else {
                0
            };
            let max_offset = min(lines[0].spans.len(), min_offset + area.width as usize);
            for line in lines.iter_mut() {
                line.spans = line.spans[min_offset..max_offset].to_vec();
            }
        }

        let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(
            paragraph,
            area.inner(&Margin {
                vertical: 1,
                horizontal: 1,
            }),
        );
        frame.render_widget(default_block(), area);
    }

    fn get_remaining_balance(&self) -> i32 {
        let planet_players = &self.planet_players[&self.planet_ids[self.planet_index]];
        let mut hiring_costs = 0 as i32;
        for (player_id, hire_cost) in planet_players.iter() {
            if !self.selected_players.contains(player_id) {
                continue;
            }
            hiring_costs += *hire_cost as i32;
        }
        let ship_cost = self.selected_ship().cost();
        INITIAL_TEAM_BALANCE as i32 - hiring_costs - ship_cost as i32
    }
    fn render_remaining_balance(&mut self, frame: &mut Frame, area: Rect) {
        let text = format!(
            " Remaining balance: {:>}{}",
            self.get_remaining_balance(),
            CURRENCY_SYMBOL
        );

        let style = if self.get_remaining_balance() >= 0 {
            UiStyle::OK
        } else {
            UiStyle::ERROR
        };
        frame.render_widget(
            Paragraph::new(text).block(default_block().border_style(style)),
            area,
        );
    }

    fn render_player_list(&mut self, frame: &mut Frame, world: &World, area: Rect) {
        if self.state < CreationState::Players {
            frame.render_widget(
                default_block()
                    .title(format!(
                        "Select {} players",
                        self.max_players_selected() - self.selected_players.len(),
                    ))
                    .style(UiStyle::UNSELECTABLE),
                area,
            );
            return;
        }

        let planet_id = self.planet_ids[self.planet_index];
        let planet_players = &self.planet_players.get(&planet_id).unwrap();
        let options = planet_players
            .iter()
            .map(|&player_data| {
                let player_id = player_data.0;
                let mut style = UiStyle::DEFAULT;
                if self.selected_players.contains(&player_id)
                    && self.state <= CreationState::Players
                {
                    style = UiStyle::OK;
                }

                if self.state > CreationState::Players
                    && !self.selected_players.contains(&player_id)
                {
                    return ("".to_string(), style);
                }
                let player = world.get_player(player_id).unwrap();

                (
                    format!(
                        "{:23} {:>5} {}",
                        format!("{} {}", player.info.first_name, player.info.last_name),
                        player.hire_cost(0.0),
                        CURRENCY_SYMBOL
                    ),
                    style,
                )
            })
            .collect_vec();

        let list = selectable_list(options, &self.callback_registry);
        let block_style = if self.state > CreationState::Players {
            UiStyle::OK
        } else {
            UiStyle::DEFAULT
        };

        let mut state = if self.state > CreationState::Players {
            ClickableListState::default().with_selected(None)
        } else {
            ClickableListState::default().with_selected(Some(self.player_index))
        };

        frame.render_stateful_widget(
            list.block(
                default_block()
                    .title(format!(
                        "Select {} players",
                        self.max_players_selected() - self.selected_players.len(),
                    ))
                    .style(block_style),
            ),
            area,
            &mut state,
        );
    }

    fn render_player(&mut self, frame: &mut Frame, world: &World, area: Rect) {
        let planet_id = self.planet_ids[self.planet_index];
        let planet_players = &self.planet_players.get(&planet_id).unwrap();
        let player = world
            .get_player(planet_players[self.player_index].0)
            .unwrap();
        render_player_description(player, &self.gif_map, self.tick, frame, world, area);
    }

    fn render_confirm_box(&mut self, frame: &mut Frame, world: &World, area: Rect) {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(area);
        let name = self.team_name_textarea.lines()[0].clone();
        let planet = world
            .get_planet_or_err(self.planet_ids[self.planet_index])
            .unwrap();
        let text = Paragraph::new(vec![
            Line::from(Span::raw(format!("{} from {}", name, planet.name))),
            Line::from(Span::raw("Ready to sail the cosmic waves?")),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(
            text,
            split[1].inner(&Margin {
                vertical: 1,
                horizontal: 1,
            }),
        );

        let side_width = if split[2].width > 24 {
            (split[2].width - 24) / 2
        } else {
            0
        };
        let button_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(side_width),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(side_width),
            ])
            .split(split[2]);

        let yes_button = Button::new(
            UiText::YES.into(),
            UiCallbackPreset::GeneratePlayerTeam {
                name: self.team_name_textarea.lines()[0].clone(),
                home_planet: self.planet_ids[self.planet_index].clone(),
                jersey_style: self.jersey_styles[self.jersey_style_index],
                jersey_colors: self.get_team_colors(),
                players: self.selected_players.clone(),
                balance: self.get_remaining_balance() as u32,
                spaceship: self.selected_ship(),
            },
            Rc::clone(&self.callback_registry),
        )
        .set_style(UiStyle::OK);
        frame.render_widget(yes_button, button_split[1]);

        let no_button = Button::new(
            UiText::NO.into(),
            UiCallbackPreset::CancelGeneratePlayerTeam,
            Rc::clone(&self.callback_registry),
        )
        .set_style(UiStyle::ERROR);

        frame.render_widget(no_button, button_split[2]);
    }

    fn max_players_selected(&self) -> usize {
        // self.selected_players.len() >= self.selected_ship().capacity() as usize
        let planet_id = self.planet_ids[self.planet_index];
        let planet_players = &self.planet_players.get(&planet_id).unwrap();
        min(INITIAL_TEAM_SIZE, planet_players.len())
    }
    fn enough_players_selected(&self) -> bool {
        self.selected_players.len() >= self.max_players_selected()
    }
}

impl Screen for NewTeamScreen {
    fn name(&self) -> &str {
        "NewTeam"
    }

    fn update(&mut self, world: &World) -> AppResult<()> {
        self.tick += 1;

        // If planets is empty, we initialize the list of planets and planet_players
        if self.planet_ids.len() == 0 {
            self.planet_ids = world
                .planets
                .keys()
                .filter(|&planet_id| {
                    let planet = world.get_planet(*planet_id).unwrap();
                    planet.total_population() > 0
                })
                .sorted_by(|a, b| a.cmp(&b))
                .map(|id| *id)
                .collect_vec();
            for player in world.players.values() {
                if player.team.is_none() {
                    let planet_players = self
                        .planet_players
                        .entry(player.info.home_planet_id)
                        .or_insert(vec![]);
                    planet_players.push((player.id, player.hire_cost(0.0)));
                    planet_players.sort_by(|a, b| {
                        let p1 = world.get_player(a.0).unwrap();
                        let p2 = world.get_player(b.0).unwrap();
                        p2.rating().cmp(&p1.rating())
                    });
                }
            }
            self.set_index(0);
        }

        Ok(())
    }
    fn render(&mut self, frame: &mut Frame, world: &World, area: Rect) -> AppResult<()> {
        let v_split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(LEFT_PANEL_WIDTH), //selections
                Constraint::Min(10),                  //planet_players
                Constraint::Length(1),
            ])
            .split(area);

        let planet_split_height = if self.state == CreationState::Planet {
            self.planet_ids.len() as u16 + 2
        } else {
            3
        };

        let jersey_split_height = if self.state == CreationState::Jersey {
            self.jersey_styles.len() as u16 + 2
        } else {
            3
        };

        let ship_split_height = if self.state == CreationState::ShipModel {
            self.spaceship_models.len() as u16 + 2
        } else {
            3
        };

        let player_split_height = if self.state >= CreationState::Players {
            self.planet_players[&self.planet_ids[self.planet_index]].len() as u16 + 2
        } else {
            3
        };

        let h_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),                   // remaining balance
                Constraint::Length(3),                   // team name
                Constraint::Length(3),                   // ship name
                Constraint::Length(planet_split_height), // planet
                Constraint::Length(4),                   // colors
                Constraint::Length(jersey_split_height), // jersey style
                Constraint::Length(ship_split_height),   // ship
                Constraint::Length(player_split_height), // player_list
                Constraint::Min(0),                      // filler
            ])
            .split(v_split[1]);

        self.render_remaining_balance(frame, h_split[0]);

        frame.render_widget(self.team_name_textarea.widget(), h_split[1]);
        if self.state == CreationState::TeamName {
            self.render_intro(frame, v_split[2]);
        }

        frame.render_widget(self.ship_name_textarea.widget(), h_split[2]);
        if self.state == CreationState::ShipName {
            self.render_intro(frame, v_split[2]);
        }

        self.render_planet_selection(frame, world, h_split[3]);
        if self.state == CreationState::Planet {
            self.render_planet(frame, world, v_split[2]);
        }

        self.render_colors_selection(frame, h_split[4]);
        self.render_jersey_selection(frame, h_split[5]);
        if self.state == CreationState::Jersey {
            self.render_jersey(frame, world, v_split[2]);
        }

        self.render_spaceship_selection(frame, h_split[6]);
        if self.state == CreationState::ShipModel {
            self.render_spaceship(frame, v_split[2]);
        }

        self.render_player_list(frame, world, h_split[7]);
        if self.state == CreationState::Players {
            self.render_player(frame, world, v_split[2]);
        }

        if self.state >= CreationState::Done {
            let width = 50;
            let height = 12;
            let x = if area.width > width {
                (area.width - width) / 2
            } else {
                0
            };
            let y = if area.height > height {
                (area.height - height) / 2
            } else {
                0
            };
            let confirm_box = Rect::new(x, y, width, height);
            frame.render_widget(Clear, confirm_box);
            self.render_confirm_box(frame, world, confirm_box);
            frame.render_widget(default_block(), confirm_box);
        }
        Ok(())
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) -> Option<UiCallbackPreset> {
        match key_event.code {
            KeyCode::Up => self.next_index(),
            KeyCode::Down => self.previous_index(),
            _ => {
                match self.state {
                    CreationState::TeamName => match key_event.code {
                        KeyCode::Enter => {
                            if !validate_textarea_input(&mut self.team_name_textarea) {
                                return None;
                            }
                            let mut name = self.team_name_textarea.lines()[0].trim().to_string();
                            // Capitalize first letter of name
                            name = name
                                .chars()
                                .enumerate()
                                .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                                .collect();

                            self.team_name_textarea.move_cursor(CursorMove::End);
                            self.team_name_textarea.delete_line_by_head();
                            self.team_name_textarea.set_yank_text(name);
                            self.team_name_textarea.paste();
                            self.team_name_textarea.set_cursor_style(UiStyle::DEFAULT);

                            self.team_name_textarea.set_block(
                                default_block().border_style(UiStyle::OK).title("Team name"),
                            );
                            self.ship_name_textarea.set_block(
                                default_block()
                                    .border_style(UiStyle::DEFAULT)
                                    .title("Ship name"),
                            );

                            self.ship_name_textarea.set_cursor_style(UiStyle::SELECTED);

                            self.set_state(self.state.next());
                        }
                        _ => {
                            self.team_name_textarea
                                .input(input_from_key_event(key_event));
                            validate_textarea_input(&mut self.team_name_textarea);
                        }
                    },
                    CreationState::ShipName => match key_event.code {
                        KeyCode::Enter => {
                            if !validate_textarea_input(&mut self.ship_name_textarea) {
                                return None;
                            }
                            let mut name = self.ship_name_textarea.lines()[0].trim().to_string();
                            // Capitalize first letter of name
                            name = name
                                .chars()
                                .enumerate()
                                .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
                                .collect();

                            self.ship_name_textarea.move_cursor(CursorMove::End);
                            self.ship_name_textarea.delete_line_by_head();
                            self.ship_name_textarea.set_yank_text(name);
                            self.ship_name_textarea.paste();
                            self.ship_name_textarea.set_cursor_style(UiStyle::DEFAULT);

                            self.ship_name_textarea.set_block(
                                default_block().border_style(UiStyle::OK).title("Ship name"),
                            );
                            self.set_state(self.state.next())
                        }
                        KeyCode::Backspace => {
                            if self.ship_name_textarea.lines()[0].is_empty() {
                                self.team_name_textarea.set_block(
                                    default_block()
                                        .border_style(UiStyle::DEFAULT)
                                        .title("Team name"),
                                );
                                self.ship_name_textarea.set_block(
                                    default_block()
                                        .border_style(UiStyle::UNSELECTABLE)
                                        .title("Ship name"),
                                );
                                self.team_name_textarea.set_cursor_style(UiStyle::SELECTED);
                                self.ship_name_textarea.set_cursor_style(UiStyle::DEFAULT);

                                self.set_state(self.state.previous());
                            } else {
                                self.ship_name_textarea
                                    .input(input_from_key_event(key_event));
                                validate_textarea_input(&mut self.ship_name_textarea);
                            }
                        }
                        _ => {
                            self.ship_name_textarea
                                .input(input_from_key_event(key_event));
                            validate_textarea_input(&mut self.ship_name_textarea);
                        }
                    },
                    CreationState::Planet => match key_event.code {
                        KeyCode::Enter => self.set_state(self.state.next()),
                        KeyCode::Backspace => {
                            self.ship_name_textarea.set_block(
                                default_block()
                                    .border_style(UiStyle::DEFAULT)
                                    .title("Ship name"),
                            );
                            self.ship_name_textarea.set_cursor_style(UiStyle::SELECTED);
                            self.set_state(self.state.previous());
                        }

                        _ => {}
                    },
                    CreationState::Jersey => match key_event.code {
                        KeyCode::Enter => {
                            self.set_state(self.state.next());
                        }
                        KeyCode::Backspace => {
                            self.set_state(self.state.previous());
                        }

                        KeyCode::Char('r') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.red_color_preset.next(),
                                channel: 0,
                            });
                        }
                        KeyCode::Char('g') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.green_color_preset.next(),
                                channel: 1,
                            });
                        }
                        KeyCode::Char('b') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.blue_color_preset.next(),
                                channel: 2,
                            });
                        }
                        _ => {}
                    },
                    CreationState::ShipModel => match key_event.code {
                        KeyCode::Enter => self.set_state(self.state.next()),
                        KeyCode::Backspace => {
                            self.set_state(self.state.previous());
                        }

                        KeyCode::Char('r') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.red_color_preset.next(),
                                channel: 0,
                            });
                        }
                        KeyCode::Char('g') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.green_color_preset.next(),
                                channel: 1,
                            });
                        }
                        KeyCode::Char('b') => {
                            return Some(UiCallbackPreset::SetTeamColors {
                                color: self.blue_color_preset.next(),
                                channel: 2,
                            });
                        }
                        _ => {}
                    },
                    CreationState::Players => match key_event.code {
                        KeyCode::Enter => {
                            let planet_id = self.planet_ids[self.planet_index];
                            let planet_players = &self.planet_players.get(&planet_id).unwrap();
                            let (player_id, _) = planet_players[self.player_index];
                            if self.selected_players.contains(&player_id) {
                                self.selected_players.retain(|&x| x != player_id);
                            } else if self.selected_players.len() < self.max_players_selected() {
                                self.selected_players.push(player_id);
                            }
                            if self.get_remaining_balance() >= 0 && self.enough_players_selected() {
                                self.set_state(self.state.next());
                            }
                        }
                        KeyCode::Backspace => {
                            self.clear_selected_players();
                            self.set_state(self.state.previous());
                        }

                        _ => {}
                    },
                    CreationState::Done => match key_event.code {
                        KeyCode::Enter => {
                            return Some(UiCallbackPreset::GeneratePlayerTeam {
                                name: self.team_name_textarea.lines()[0].clone(),
                                home_planet: self.planet_ids[self.planet_index].clone(),
                                jersey_style: self.jersey_styles[self.jersey_style_index],
                                jersey_colors: self.get_team_colors(),
                                players: self.selected_players.clone(),
                                balance: self.get_remaining_balance() as u32,
                                spaceship: self.selected_ship().clone(),
                            });
                        }
                        KeyCode::Backspace => {
                            self.set_index(0);
                            return Some(UiCallbackPreset::CancelGeneratePlayerTeam);
                        }
                        KeyCode::Left => {
                            self.confirm = ConfirmChoice::Yes;
                        }
                        KeyCode::Right => {
                            self.confirm = ConfirmChoice::No;
                        }
                        _ => {}
                    },
                }
            }
        }

        None
    }
}

impl SplitPanel for NewTeamScreen {
    fn index(&self) -> usize {
        match self.state {
            CreationState::Planet => self.planet_index,
            CreationState::Jersey => self.jersey_style_index,
            CreationState::ShipModel => self.spaceship_model_index,
            CreationState::Players => self.player_index,
            _ => 0,
        }
    }

    fn max_index(&self) -> usize {
        match self.state {
            CreationState::Planet => self.planet_ids.len(),
            CreationState::Jersey => self.jersey_styles.len(),
            CreationState::ShipModel => self.spaceship_models.len(),
            CreationState::Players => {
                let planet_id = self.planet_ids[self.planet_index];
                let planet_players = &self.planet_players.get(&planet_id).unwrap();
                planet_players.len()
            }
            _ => 0,
        }
    }

    fn set_index(&mut self, index: usize) {
        match self.state {
            CreationState::Planet => {
                self.planet_index = index;
            }
            CreationState::Jersey => {
                self.jersey_style_index = index;
            }
            CreationState::ShipModel => {
                self.spaceship_model_index = index;
            }
            CreationState::Players => {
                self.player_index = index;
            }
            _ => {}
        }
    }
}

fn validate_textarea_input(textarea: &mut TextArea<'_>) -> bool {
    let text = textarea.lines()[0].trim();
    if text.len() < MIN_NAME_LENGTH {
        textarea.set_style(UiStyle::ERROR);
        false
    } else if text.len() > MAX_NAME_LENGTH {
        textarea.set_style(UiStyle::ERROR);
        false
    } else {
        textarea.set_style(UiStyle::DEFAULT);
        true
    }
}
