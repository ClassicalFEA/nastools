//! This module implements the top-level GUI for `nastester`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt::Debug;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::str::FromStr;

use egui::{
  Align, Color32, ComboBox, Context, DragValue, FontFamily, Id, Layout,
  RichText, ScrollArea, TextStyle, Ui, Visuals, WidgetText,
};
use egui_extras::{Column, TableBuilder};
use f06::blocks::types::BlockType;
use f06::prelude::*;
use log::*;
use nas_csv::formatting::FloatFormat;
use native_dialog::{MessageDialog, MessageType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::*;
use crate::results::*;
use crate::running::*;
use crate::suite::*;

/// This enum contains the different views that can be rendered.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) enum View {
  /// Default view: the decks.
  #[default]
  Decks,
  /// The solvers.
  Solvers,
  /// The criteria sets.
  CriteriaSets,
  /// A specific deck's extractions.
  Extractions,
  /// A deck's side-by-side results.
  Results,
}

/// This contains form fields hat are always present.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StaticFields {
  /// The deck whose results we're looking at.
  current_deck: Option<Uuid>,
  /// The currently-selected results blockref.
  block_ref: Option<BlockRef>,
  /// Limit display to values that are part of extractions?
  extractions_only: bool,
  /// Highlight flagged values?
  highlight_flagged: bool,
}

impl Default for StaticFields {
  fn default() -> Self {
    return Self {
      current_deck: None,
      block_ref: None,
      extractions_only: false,
      highlight_flagged: true,
    };
  }
}

/// This struct rerpresents the GUI.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Gui {
  /// The inner app state.
  pub(crate) state: AppState,
  /// The current view.
  pub(crate) view: View,
  /// Whether we have a save file for the test suite.
  pub(crate) suite_file: Option<PathBuf>,
  /// Whether the current suite has been saved.
  pub(crate) suite_clean: bool,
  /// Text fields that are not 1:1 with state data, so they need to stay
  /// "invalid" sometimes. These are cleared when the view changes.
  pub(crate) text_fields: HashMap<Id, String>,
  /// Fields that are always present.
  pub(crate) static_fields: StaticFields,
}

impl Default for Gui {
  fn default() -> Self {
    return Self {
      state: AppState::default(),
      view: View::Decks,
      suite_file: None,
      suite_clean: true,
      text_fields: HashMap::new(),
      static_fields: StaticFields::default(),
    };
  }
}

/// Fallible function for GUI inner stuff.
type GuiFn<T> = fn(&mut Gui, &mut Ui) -> Result<T, Box<dyn Error>>;

impl Gui {
  /// Constructs a new Gui with an eframe creation context.
  pub(crate) fn new(_cc: &eframe::CreationContext<'_>) -> Self {
    Self::default()
  }

  /// Save the suite. Returns whether the save happened.
  fn save_suite(&mut self, _ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    if self.suite_file.is_none() {
      // show dialog
      let picked = rfd::FileDialog::new()
        .add_filter("nastester suite file", &[SUITE_FILE_EXTENSION])
        .set_title("Save suite to file...")
        .set_can_create_directories(true)
        .save_file();
      if let Some(mut p) = picked {
        if p.extension().is_none() {
          p.set_extension(SUITE_FILE_EXTENSION);
        }
        self.suite_file = Some(p);
      }
    }
    if let Some(ref p) = self.suite_file {
      let file = File::create(p)?;
      let mut writer = BufWriter::new(file);
      serde_json::to_writer_pretty(&mut writer, &self.state.suite)?;
      writer.flush()?;
      self.suite_clean = true;
      log::info!("Saved suite to {}.", p.display());
      return Ok(true);
    } else {
      log::info!("Suite saving cancelled or no file chosen.");
      return Ok(false);
    }
  }

  /// Ensure the suite is clean before changing paths of closing. Returns
  /// whether the cleanliness was ensured.
  fn sure_clean(&mut self, ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    if !self.suite_clean {
      let wants_save = MessageDialog::new()
        .set_title("Are you sure?")
        .set_text("You have unsaved changes. Do you want to save them before?")
        .show_confirm()
        .unwrap_or(true);
      if wants_save {
        self.save_suite(ui)?;
      }
    }
    return Ok(self.suite_clean);
  }

  /// "Save as" -- clear the save path and save, basically.
  fn save_suite_as(&mut self, ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    self.sure_clean(ui)?;
    self.suite_clean = false;
    self.suite_file = None;
    return self.save_suite(ui);
  }

  /// Load a suite. Returns whether the load happened.
  fn load_suite(&mut self, ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    self.sure_clean(ui)?;
    self.suite_file = rfd::FileDialog::new()
      .add_filter("nastester suite file", &[SUITE_FILE_EXTENSION])
      .add_filter("All files", &["*"])
      .set_title("Load suite from file...")
      .set_can_create_directories(true)
      .pick_file();
    if let Some(ref p) = self.suite_file {
      let file = File::open(p)?;
      let reader = BufReader::new(file);
      self.state.suite = serde_json::from_reader(reader)?;
      log::info!("Loaded suite from {}.", p.display());
      return Ok(true);
    }
    log::info!("Suite loading cancelled or no file chosen.");
    return Ok(false);
  }

  /// Add one or more decks. Returns how many.
  fn add_decks(&mut self, _ui: &mut Ui) -> Result<usize, Box<dyn Error>> {
    let deck_files = rfd::FileDialog::new()
      .add_filter("NASTRAN input files", DECK_EXTENSIONS)
      .add_filter("All files", &["*"])
      .set_title("Choose input files...")
      .set_can_create_directories(true)
      .pick_files();
    if let Some(v) = deck_files {
      let mut total = 0;
      for in_file in v {
        if in_file.is_file() {
          log::info!("Added deck from file {}.", in_file.display());
          self.suite_clean = false;
          self.state.add_deck(in_file);
          total += 1;
        } else {
          log::info!("Tried to add deck from non-file {}!", in_file.display());
        }
      }
      return Ok(total);
    } else {
      log::info!("Deck addition cancelled by user or no file(s) selected.");
      return Ok(0);
    }
  }

  /// Fila dialog to pick a solver binary.
  fn solver_bin_dialog() -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
      .set_title("Choose solver binary...")
      .set_can_create_directories(true);
    #[allow(clippy::const_is_empty)]
    if !BINARY_EXTENSIONS.is_empty() {
      dialog = dialog.add_filter("Executable file", BINARY_EXTENSIONS);
    }
    return dialog.pick_file();
  }

  /// Add a solver binary. Returns whether it's been added.
  fn add_solver_bin(&mut self, _ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    let binary = Self::solver_bin_dialog();
    if let Some(bin) = binary {
      log::info!("Added solver binary {}.", bin.display());
      self.state.add_solver_bin(bin);
      return Ok(true);
    } else {
      log::info!("Solver addition cancelled by user or no binary selected.");
      return Ok(false);
    }
  }

  /// Dialog to pick an F06 directory.
  fn solver_dir_dialog() -> Option<PathBuf> {
    return rfd::FileDialog::new()
      .set_title("Choose F06 directory...")
      .set_can_create_directories(true)
      .pick_folder();
  }

  /// Add a solver as an F06 directory. Returns whether it's been added.
  fn add_solver_dir(&mut self, _ui: &mut Ui) -> Result<bool, Box<dyn Error>> {
    let directory = Self::solver_dir_dialog();
    if let Some(dir) = directory {
      log::info!("Added solver as F06 directory {}.", dir.display());
      self.state.add_solver_dir(dir);
      return Ok(true);
    } else {
      log::info!("Solver addition cancelled by user or no folder selected.");
      return Ok(false);
    }
  }

  /// Change a deck's file path. Returns whether it's been changed.
  fn change_deck(&mut self, deck: Uuid) -> Result<bool, Box<dyn Error>> {
    let deck_file = rfd::FileDialog::new()
      .add_filter("NASTRAN input files", DECK_EXTENSIONS)
      .add_filter("All files", &["*"])
      .set_title("Choose input files...")
      .set_can_create_directories(true)
      .pick_file();
    if let Some(v) = deck_file {
      if let Some(d) = self.state.suite.decks.get_mut(&deck) {
        log::info!(
          "Deck {} path changed from {} to {}.",
          deck,
          d.in_file.display(),
          v.display()
        );
        d.in_file = v;
        self.suite_clean = false;
        return Ok(true);
      }
      log::warn!("Tried to change path for non-existing deck!");
      return Ok(false);
    } else {
      log::info!("Deck addition cancelled by user or no file(s) selected.");
      return Ok(false);
    }
  }

  /// Change a solver's path. Returns whether it's been changed.
  fn change_solver(&mut self, solver: Uuid) -> Result<bool, Box<dyn Error>> {
    let current = self.state.solvers.get_mut(&solver).unwrap();
    match current.method {
      RunMethod::ImportFromDir(ref mut p) => {
        if let Some(np) = Self::solver_dir_dialog() {
          *p = np;
          return Ok(true);
        }
      }
      RunMethod::RunSolver(ref mut p) => {
        if let Some(np) = Self::solver_bin_dialog() {
          *p = np;
          return Ok(true);
        }
      }
    };
    return Ok(false);
  }

  /// Run a function and, if an error happens, do a pop-up.
  fn show_error(err: Box<dyn Error>) {
    MessageDialog::new()
      .set_title("Error")
      .set_type(MessageType::Error)
      .set_text(err.to_string().as_str())
      .show_alert()
      .ok();
  }

  /// Try to run an inner subroutine and, if it fails, show a pop-up.
  fn try_run<T>(&mut self, ui: &mut Ui, f: GuiFn<T>) {
    if let Err(err) = f(self, ui) {
      Self::show_error(err);
    }
  }

  /// Returns an editable text field buffer.
  fn text_buffer(&mut self, id: Id) -> &mut String {
    self.text_fields.entry(id).or_default();
    return self.text_fields.get_mut(&id).unwrap();
  }

  /// Clears all temp field buffers.
  fn clear_buffers(&mut self) {
    self.text_fields.clear();
  }

  /// Changes the view, and clears text buffers.
  fn switch_to(&mut self, view: View) {
    self.view = view;
    self.clear_buffers();
  }

  /// Editable list of text-convertible things.
  fn editable_vec<F, T: Clone + PartialEq + ToString + FromStr>(
    &mut self,
    ui: &mut Ui,
    finder: F,
  ) where
    <T as FromStr>::Err: Debug,
    F: Fn(&mut Self) -> &mut Vec<T>,
  {
    let body_height = ui.text_style_height(&TextStyle::Body);
    let field_id = ui.next_auto_id();
    TableBuilder::new(ui)
      .vscroll(false)
      .auto_shrink(true)
      .striped(false)
      .column(Column::auto())
      .column(Column::remainder().resizable(true))
      .body(|body| {
        body.rows(body_height, finder(self).len() + 1, |mut row| {
          let i = row.index();
          if i < finder(self).len() {
            let x = finder(self).get(i).unwrap();
            row.col(|ui| {
              ui.label(x.to_string());
            });
            row.col(|ui| {
              if ui.button("x").clicked() {
                finder(self).remove(i);
              }
            });
          } else {
            row.col(|ui| {
              ui.text_edit_singleline(self.text_buffer(field_id));
            });
            row.col(|ui| {
              if ui.button("add").clicked() {
                if let Ok(k) = self.text_buffer(field_id).parse::<T>() {
                  finder(self).push(k);
                }
                self.text_buffer(field_id).clear();
              }
            });
          }
        });
      });
  }

  /// Editable list of values in a set.
  fn comboable_vec<F, T: Clone + PartialEq + ToString>(
    &mut self,
    ui: &mut Ui,
    set: &'static [T],
    vec_finder: F,
  ) where
    F: Fn(&mut Self) -> &mut Vec<T>,
  {
    let body_height = ui.text_style_height(&TextStyle::Body);
    TableBuilder::new(ui)
      .vscroll(false)
      .auto_shrink(true)
      .striped(false)
      .column(Column::auto())
      .column(Column::remainder().resizable(true))
      .body(|body| {
        body.rows(body_height, vec_finder(self).len() + 1, |mut row| {
          let i = row.index();
          if i < vec_finder(self).len() {
            let x = vec_finder(self).get_mut(i).unwrap();
            row.col(|ui| {
              ComboBox::from_id_source(ui.next_auto_id())
                .selected_text(x.to_string())
                .show_ui(ui, |ui| {
                  for val in set {
                    ui.selectable_value(
                      x,
                      val.clone(),
                      val.to_string().as_str(),
                    );
                  }
                });
            });
            row.col(|ui| {
              if ui.button("x").clicked() {
                vec_finder(self).remove(i);
                self.suite_clean = false;
              }
            });
          } else {
            row.col(|_ui| {});
            row.col(|ui| {
              if ui.button("add").clicked() {
                vec_finder(self).push(set[0].clone());
                self.suite_clean = false;
              }
            });
          }
        });
      });
  }

  /// Aux function to render text-inserted specifier inputs.
  fn text_specifier<F, T: Clone + PartialEq + ToString + FromStr>(
    &mut self,
    ui: &mut Ui,
    finder: F,
  ) where
    <T as FromStr>::Err: Debug,
    F: Fn(&mut Self) -> &mut Specifier<T>,
  {
    ui.horizontal(|ui| {
      ComboBox::from_id_source(ui.next_auto_id())
        .selected_text(format!("{}", finder(self).get_type()))
        .show_ui(ui, |ui| {
          let tgt = finder(self);
          let types = [
            SpecifierType::All,
            SpecifierType::List,
            SpecifierType::AllExcept,
          ];
          for new_type in types {
            ui.selectable_value(tgt, tgt.with_type(new_type), new_type.name());
          }
        });
      match finder(self) {
        Specifier::All => {}
        Specifier::List(_) | Specifier::AllExcept(_) => {
          self.editable_vec(ui, |s| finder(s).inner_vec_mut().unwrap());
        }
      };
    });
  }

  /// Aux function to render combo-inserted specifier inputs.
  fn combo_specifier<F, T: Clone + PartialEq + ToString>(
    &mut self,
    ui: &mut Ui,
    set: &'static [T],
    spec_finder: F,
  ) where
    F: Fn(&mut Self) -> &mut Specifier<T>,
  {
    ui.horizontal(|ui| {
      ComboBox::from_id_source(ui.next_auto_id())
        .selected_text(format!("{}", spec_finder(self).get_type()))
        .show_ui(ui, |ui| {
          let tgt = spec_finder(self);
          let types = [
            SpecifierType::All,
            SpecifierType::List,
            SpecifierType::AllExcept,
          ];
          for new_type in types {
            ui.selectable_value(tgt, tgt.with_type(new_type), new_type.name());
          }
        });
      match spec_finder(self) {
        Specifier::All => {}
        Specifier::List(_) | Specifier::AllExcept(_) => {
          self.comboable_vec(ui, set, |s| {
            spec_finder(s).inner_vec_mut().unwrap()
          });
        }
      };
    });
  }

  /// Render function for the menu bar.
  fn show_menu(&mut self, ctx: &Context, ui: &mut Ui) {
    egui::menu::bar(ui, |ui| {
      // suite menu
      ui.menu_button("Suite", |ui| {
        if ui.button("New").clicked() {
          self.state = AppState::default();
          self.suite_clean = true;
          self.suite_file = None;
        }
        if ui.button("Save").clicked() {
          self.try_run(ui, Gui::save_suite);
        }
        if ui.button("Save as...").clicked() {
          self.try_run(ui, Gui::save_suite_as);
        }
        if ui.button("Load").clicked() {
          self.try_run(ui, Gui::load_suite);
        }
      });
      // decks menu
      ui.menu_button("Decks", |ui| {
        if ui.button("View/edit decks").clicked() {
          self.switch_to(View::Decks);
        }
        if ui.button("Add...").clicked() {
          self.try_run(ui, Gui::add_decks);
        }
      });
      // generates sub-menu solver buttons
      let btn = |state: &mut AppState,
                 ui: &mut Ui,
                 opt: Option<Uuid>,
                 lbl: &str,
                 pick: SolverPick| {
        let mut rt = RichText::new(lbl);
        let tgt = match pick {
          SolverPick::Reference => &mut state.runner.ref_solver,
          SolverPick::Testing => &mut state.runner.test_solver,
        };
        if opt == *tgt {
          rt = rt.strong();
        }
        if ui.button(rt).clicked() && opt != *tgt {
          *tgt = opt;
          state.clear_results_of(pick);
        }
      };
      // solvers menu
      ui.menu_button("Solvers", |ui| {
        if ui.button("View/edit solvers").clicked() {
          self.switch_to(View::Solvers);
        }
        if ui.button("Add solver binary...").clicked() {
          self.try_run(ui, Gui::add_solver_bin);
        }
        if ui.button("Add F06 directory...").clicked() {
          self.try_run(ui, Gui::add_solver_dir);
        }
        let snames = self
          .state
          .solvers_names()
          .map(|(s, u)| (s.to_owned(), u))
          .collect::<Vec<_>>();
        ui.menu_button("Set reference solver", |ui| {
          btn(&mut self.state, ui, None, "<none>", SolverPick::Reference);
          for (s, u) in snames.iter() {
            btn(
              &mut self.state,
              ui,
              Some(*u),
              s.as_str(),
              SolverPick::Reference,
            );
          }
        });
        ui.menu_button("Set solver under test", |ui| {
          btn(&mut self.state, ui, None, "<none>", SolverPick::Testing);
          for (s, u) in snames.iter() {
            btn(
              &mut self.state,
              ui,
              Some(*u),
              s.as_str(),
              SolverPick::Testing,
            );
          }
        });
      });
      // criteria sets menu
      ui.menu_button("Criteria sets", |ui| {
        if ui.button("Edit criteria sets").clicked() {
          self.switch_to(View::CriteriaSets);
        }
      });
      // run menu
      ui.menu_button("Run!", |ui| {
        if ui.button("Run all on both solvers").clicked() {
          self.state.enqueue_solver(SolverPick::Reference);
          self.state.enqueue_solver(SolverPick::Testing);
          self.state.run_queue();
        }
        if ui.button("Run all on reference solver").clicked() {
          self.state.enqueue_solver(SolverPick::Reference);
          self.state.run_queue();
        }
        if ui.button("Run all on solver under test").clicked() {
          self.state.enqueue_solver(SolverPick::Testing);
          self.state.run_queue();
        }
      });
      // advanced stuff
      ui.menu_button("Advanced", |ui| {
        // dark mode toggler
        let dark_mode = ctx.style().visuals.dark_mode;
        if dark_mode {
          if ui.button("Change to light mode").clicked() {
            ctx.set_visuals(Visuals::light());
          }
        } else if ui.button("Change to dark mode").clicked() {
          ctx.set_visuals(Visuals::dark());
        }
        // recompute flags
        if ui.button("Recompute flags").clicked() {
          self.state.recompute_all_flagged();
        }
        // dump app state
        if ui.button("Dump app state").clicked() {
          info!("User-requested dump of app state:\n{self:#?}");
        }
        // toggle gui debug
        #[cfg(debug_assertions)]
        {
          let guidebug = if ctx.debug_on_hover() { "ON" } else { "OFF" };
          if ui.button(format!("GUI debug {guidebug}")).clicked() {
            ctx.set_debug_on_hover(!ctx.debug_on_hover());
          }
        }
      });
    });
  }

  /// Render function for the global decks list.
  fn view_decks(&mut self, ctx: &Context) {
    // one per deck
    let deck_data = self
      .state
      .decks_by_name()
      .map(|(u, d, r)| (u, d.clone(), r))
      .collect::<Vec<_>>();
    egui::CentralPanel::default().show(ctx, |ui| {
      self.show_menu(ctx, ui);
      let heading_height = ui.text_style_height(&TextStyle::Heading);
      let dy = ui.spacing().item_spacing.y;
      let body_height = ui.text_style_height(&TextStyle::Body) + dy;
      let mut cells = Layout::left_to_right(Align::Center);
      cells.main_wrap = false;
      let ndecks = deck_data.len();
      if deck_data.is_empty() {
        ui.columns(3, |cols| {
          cols[1].horizontal_centered(|ui| {
            egui::Grid::new("no_decks_grid").show(ui, |ui| {
              ui.strong("No decks in current suite.");
              ui.end_row();
              ui.horizontal(|ui| {
                ui.label("Maybe");
                if ui.button("add some").clicked() {
                  self.try_run(ui, Gui::add_decks);
                }
                ui.label("or");
                if ui.button("load a suite file").clicked() {
                  self.try_run(ui, Gui::load_suite);
                }
                ui.label("?");
              });
              ui.end_row();
            })
          });
        });
      } else {
        ui.vertical_centered(|ui| {
          ui.strong("Decks in current suite:");
        });
        TableBuilder::new(ui)
          .vscroll(true)
          .auto_shrink(true)
          .striped(true)
          .cell_layout(cells)
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .header(heading_height, |mut header| {
            header.col(|ui| {
              ui.heading("File name");
            });
            header.col(|ui| {
              ui.heading("Status");
            });
            header.col(|ui| {
              ui.heading("Reference run");
            });
            header.col(|ui| {
              ui.heading("Test run");
            });
            header.col(|ui| {
              ui.heading("Flagged");
            });
            header.col(|ui| {
              ui.heading("Actions");
            });
          })
          .body(|body| {
            body.rows(body_height, ndecks, |mut row| {
              let (uuid, deck, results) = deck_data.get(row.index()).unwrap();
              // filename
              row.col(|ui| {
                ui.label(deck.name());
              });
              // status
              row.col(|ui| {
                if deck.in_file.is_file() {
                  ui.label("Ready");
                } else {
                  ui.add(egui::Label::new(
                    WidgetText::from("Missing!").strong().color(Color32::RED),
                  ));
                  if ui.button("Locate...").clicked() {
                    self.change_deck(*uuid).ok();
                  }
                }
              });
              // results
              let mut lblres = |ui: &mut Ui, res: &RunState, p: SolverPick| {
                let (text, color) = match res {
                  RunState::Ready => {
                    if ui.button("Run").clicked() {
                      self.state.enqueue_deck_safe(*uuid, p);
                      self.state.run_queue();
                    }
                    return;
                  }
                  RunState::Enqueued => {
                    ("In queue".to_owned(), Color32::LIGHT_YELLOW)
                  }
                  RunState::Running => ("Running".to_owned(), Color32::YELLOW),
                  RunState::Finished(_) => {
                    ("Finished".to_owned(), Color32::DARK_GREEN)
                  }
                  RunState::Error(e) => (format!("Error: {e}"), Color32::RED),
                };
                ui.add(egui::Label::new(WidgetText::from(text).color(color)));
              };
              if let Some(res) = results {
                if let Ok(h) = res.try_lock() {
                  // got lock on results
                  // reference run
                  row.col(|ui| lblres(ui, &h.ref_f06, SolverPick::Reference));
                  // test run
                  row.col(|ui| lblres(ui, &h.test_f06, SolverPick::Testing));
                  // flags
                  match (&h.ref_f06, &h.test_f06) {
                    (RunState::Finished(_), RunState::Finished(_)) => {
                      row.col(|ui| {
                        ui.label(format!("{} values", h.num_flagged()));
                      });
                    }
                    _ => {
                      row.col(|ui| {
                        ui.label("(requires both runs)");
                      });
                    }
                  };
                } else {
                  // no lock on results
                  // reference run
                  row.col(|ui| {
                    lblres(ui, &RunState::Running, SolverPick::Reference)
                  });
                  // test run
                  row.col(|ui| {
                    lblres(ui, &RunState::Running, SolverPick::Testing)
                  });
                  // flags
                  row.col(|ui| {
                    ui.label("(running)");
                  });
                }
              } else {
                // no results, so it's just ready
                // reference run
                row.col(|ui| {
                  lblres(ui, &RunState::Ready, SolverPick::Reference)
                });
                // test run
                row.col(|ui| lblres(ui, &RunState::Ready, SolverPick::Testing));
                // flags
                row.col(|ui| {
                  ui.label("(requires both runs)");
                });
              }
              // actions
              row.col(|ui| {
                ui.horizontal(|ui| {
                  if ui.button("Edit extractions").clicked() {
                    self.static_fields.current_deck = Some(*uuid);
                    self.switch_to(View::Extractions);
                  }
                  if ui.button("View results").clicked() {
                    self.static_fields.current_deck = Some(*uuid);
                    self.switch_to(View::Results);
                  }
                  if ui.button("Change file path").clicked() {
                    self.change_deck(*uuid).ok();
                  }
                  if ui.button("Remove").clicked() {
                    self.state.suite.decks.remove(uuid);
                    self.suite_clean = false;
                  }
                });
              });
            });
          });
      }
    });
  }

  /// Render function for a single deck, its extractions, etcetera.
  fn view_deck_exns(&mut self, ctx: &Context) {
    let uuid = self.static_fields.current_deck.expect("missing deck UUID");
    if self.state.suite.decks.contains_key(&uuid) {
      let exns_ui = |ui: &mut Ui| {
        self.show_menu(ctx, ui);
        ui.vertical_centered(|ui| {
          let sf = &mut self.static_fields;
          let dn = self.state.get_deck(uuid).unwrap().0.name().to_owned();
          let deck_data = self.state.decks_names().collect::<Vec<(_, _)>>();
          ComboBox::from_id_source("res_deck_picker")
            .selected_text(dn)
            .show_ui(ui, |ui| {
              for (s, u) in deck_data {
                ui.selectable_value(&mut sf.current_deck, Some(u), s);
              }
            });
          ui.strong("Deck extractions:");
          if ui.button("Add new").clicked() {
            self
              .state
              .get_deck_mut(uuid)
              .expect("deck UUID missing for extraction addition")
              .0
              .extractions
              .push((Extraction::default(), None));
          }
          let heading_height = ui.text_style_height(&TextStyle::Heading);
          let body_height = ui.text_style_height(&TextStyle::Body);
          let item_height = body_height + ui.spacing().item_spacing.y;
          TableBuilder::new(ui)
            .vscroll(true)
            .auto_shrink(true)
            .striped(true)
            .column(Column::auto())
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .column(Column::remainder().resizable(true))
            .header(heading_height, |mut header| {
              header.col(|ui| {
                ui.label("#");
              });
              header.col(|ui| {
                ui.label("blocks");
              });
              header.col(|ui| {
                ui.label("subcases");
              });
              header.col(|ui| {
                ui.label("nodes");
              });
              header.col(|ui| {
                ui.label("elements");
              });
              header.col(|ui| {
                ui.label("columns");
              });
              header.col(|ui| {
                ui.label("on disjunction");
              });
              header.col(|ui| {
                ui.label("criteria");
              });
            })
            .body(|mut body| {
              let (deck_ref, _results_ref) = self
                .state
                .get_deck_mut(uuid)
                .expect("deck UUID missing for extraction addition");
              let exns = deck_ref.extractions.clone();
              for (i, (exn, crit)) in exns.iter().enumerate() {
                // estimate height of the row based on the extraction with the
                // longest inner vector
                let max_exn_lens = [
                  exn.block_types.inner_vec().map_or(0, |v| v.len()),
                  exn.subcases.inner_vec().map_or(0, |v| v.len()),
                  exn.grid_points.inner_vec().map_or(0, |v| v.len()),
                  exn.elements.inner_vec().map_or(0, |v| v.len()),
                ]
                .into_iter()
                .max()
                .unwrap()
                  + 1;
                let est_height = max_exn_lens as f32 * item_height;
                body.row(est_height, |mut row| {
                  row.col(|ui| {
                    ui.label(i.to_string());
                  });
                  row.col(|ui| {
                    self.combo_specifier(ui, BlockType::all(), |s| {
                      &mut s
                        .state
                        .suite
                        .decks
                        .get_mut(&uuid)
                        .expect("deck UUID missing!")
                        .extractions
                        .get_mut(i)
                        .expect("bad extraction index!")
                        .0
                        .block_types
                    });
                  });
                  row.col(|ui| {
                    self.text_specifier(ui, |s| {
                      &mut s
                        .state
                        .suite
                        .decks
                        .get_mut(&uuid)
                        .expect("deck UUID missing!")
                        .extractions
                        .get_mut(i)
                        .expect("bad extraction index!")
                        .0
                        .subcases
                    });
                  });
                  row.col(|ui| {
                    self.text_specifier(ui, |s| {
                      &mut s
                        .state
                        .suite
                        .decks
                        .get_mut(&uuid)
                        .expect("deck UUID missing!")
                        .extractions
                        .get_mut(i)
                        .expect("bad extraction index!")
                        .0
                        .grid_points
                    });
                  });
                  row.col(|ui| {
                    self.text_specifier(ui, |s| {
                      &mut s
                        .state
                        .suite
                        .decks
                        .get_mut(&uuid)
                        .expect("deck UUID missing!")
                        .extractions
                        .get_mut(i)
                        .expect("bad extraction index!")
                        .0
                        .elements
                    });
                  });
                  row.col(|ui| {
                    self.text_specifier(ui, |s| {
                      &mut s
                        .state
                        .suite
                        .decks
                        .get_mut(&uuid)
                        .expect("deck UUID missing!")
                        .extractions
                        .get_mut(i)
                        .expect("bad extraction index!")
                        .0
                        .raw_cols
                    });
                  });
                  row.col(|ui| {
                    let dxn = &mut self
                      .state
                      .suite
                      .decks
                      .get_mut(&uuid)
                      .expect("deck UUID missing!")
                      .extractions
                      .get_mut(i)
                      .expect("bad extraction index!")
                      .0
                      .dxn;
                    ComboBox::from_id_source(ui.next_auto_id())
                      .selected_text(dxn.to_string())
                      .show_ui(ui, |ui| {
                        let all = [
                          DisjunctionBehaviour::AssumeZeroes,
                          DisjunctionBehaviour::Skip,
                          DisjunctionBehaviour::Flag,
                        ];
                        for db in all {
                          ui.selectable_value(dxn, db, db.to_string());
                        }
                      });
                  });
                  row.col(|ui| {
                    ComboBox::from_id_source(ui.next_auto_id())
                      .selected_text(crit.map_or("<none>".to_owned(), |u| {
                        self
                          .state
                          .suite
                          .criteria_sets
                          .get(&u)
                          .map(|c| c.name.clone())
                          .expect("critset UUID missing")
                      }))
                      .show_ui(ui, |ui| {
                        let crit_mut = &mut self
                          .state
                          .suite
                          .decks
                          .get_mut(&uuid)
                          .unwrap()
                          .extractions
                          .get_mut(i)
                          .unwrap()
                          .1;
                        ui.selectable_value(crit_mut, None, "<none>");
                        let critsets = self.state.suite.criteria_sets.iter();
                        for (uuid, crit) in critsets {
                          ui.selectable_value(
                            crit_mut,
                            Some(*uuid),
                            &crit.name,
                          );
                        }
                      });
                  });
                })
              }
            })
        })
      };
      egui::CentralPanel::default().show(ctx, exns_ui);
    } else {
      egui::CentralPanel::default().show(ctx, |ui| {
        self.show_menu(ctx, ui);
        log::error!("Tried to go to deck with invalid UUID!");
        ui.label("Invalid deck UUID!");
      });
    }
  }

  /// Render function for the criteria set list.
  fn view_criteria_sets(&mut self, ctx: &Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
      self.show_menu(ctx, ui);
      let heading_height = ui.text_style_height(&TextStyle::Heading);
      let body_height =
        ui.text_style_height(&TextStyle::Body) + ui.spacing().item_spacing.y;
      let mut cells = Layout::left_to_right(Align::Center);
      cells.main_wrap = false;
      if self.state.suite.criteria_sets.is_empty() {
        ui.columns(3, |cols| {
          cols[1].horizontal_centered(|ui| {
            egui::Grid::new("no_critsets_grid").show(ui, |ui| {
              ui.strong("No criteria sets in current suite.");
              ui.end_row();
              ui.horizontal(|ui| {
                ui.label("Maybe");
                if ui.button("add one").clicked() {
                  self.state.add_crit_set();
                }
                ui.label("or");
                if ui.button("load a suite file").clicked() {
                  self.try_run(ui, Gui::load_suite);
                }
                ui.label("?");
              });
              ui.end_row();
            })
          });
        });
      } else {
        let mut names_ids = self
          .state
          .suite
          .criteria_sets
          .iter()
          .map(|(u, c)| (c.name.clone(), *u))
          .collect::<Vec<(_, _)>>();
        let nsets = self.state.suite.criteria_sets.len();
        names_ids.sort_by(|a, b| a.0.cmp(&b.0));
        ui.vertical_centered(|ui| {
          ui.strong("Criteria sets in current suite:");
          if ui.button("Add new").clicked() {
            self.state.add_crit_set();
          }
        });
        TableBuilder::new(ui)
          .vscroll(true)
          .auto_shrink(false)
          .striped(true)
          .cell_layout(cells)
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto())
          .column(Column::auto())
          .column(Column::auto())
          .column(Column::auto())
          .header(heading_height, |mut header| {
            header.col(|ui| {
              ui.heading("Criteria set name");
            });
            header.col(|ui| {
              ui.heading("Max absolute difference");
            });
            header.col(|ui| {
              ui.heading("Max absolute ratio");
            });
            header.col(|ui| {
              ui.heading("Flag NaN");
            });
            header.col(|ui| {
              ui.heading("Flag infinities");
            });
            header.col(|ui| {
              ui.heading("Flag if signs differ");
            });
            header.col(|ui| {
              ui.heading("Actions");
            });
          })
          .body(|body| {
            body.rows(body_height, nsets, |mut row| {
              let uuid = names_ids.get(row.index()).unwrap().1;
              let critset = self
                .state
                .suite
                .criteria_sets
                .get_mut(&uuid)
                .expect("unable to find critset");
              // name
              row.col(|ui| {
                ui.text_edit_singleline(&mut critset.name);
              });
              // disable-able number
              let disableable_number = |ui: &mut Ui, n: &mut Option<f64>| {
                if let Some(ref mut x) = n {
                  let drag = DragValue::new(x).speed(0.1);
                  ui.add(drag);
                  if ui.button("disable").clicked() {
                    *n = None;
                  }
                } else if ui.button("enable").clicked() {
                  *n = Some(1.0);
                }
              };
              // max abs diff
              row.col(|ui| {
                disableable_number(ui, &mut critset.criteria.difference);
              });
              // max ratio
              row.col(|ui| {
                disableable_number(ui, &mut critset.criteria.ratio);
              });
              // flag NaNs
              row.col(|ui| {
                ui.vertical_centered(|ui| {
                  ui.checkbox(&mut critset.criteria.nan, "");
                });
              });
              // flag NaNs
              row.col(|ui| {
                ui.vertical_centered(|ui| {
                  ui.checkbox(&mut critset.criteria.inf, "");
                });
              });
              // flag differing signals
              row.col(|ui| {
                ui.vertical_centered(|ui| {
                  ui.checkbox(&mut critset.criteria.sig, "");
                });
              });
              // delete action
              row.col(|ui| {
                if ui.button("Delete").clicked() {
                  self.state.delete_crit_set(uuid);
                }
              });
            });
          });
      }
    });
  }

  /// Render function for the solvers.
  fn view_solvers(&mut self, ctx: &Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
      self.show_menu(ctx, ui);
      let heading_height = ui.text_style_height(&TextStyle::Heading);
      let dy = ui.spacing().item_spacing.y;
      let body_height = ui.text_style_height(&TextStyle::Body) + dy;
      let mut cells = Layout::left_to_right(Align::Center);
      let nsolvers = self.state.solvers.len();
      let mut snames = self
        .state
        .solvers_names()
        .map(|(s, u)| (s.to_owned(), u))
        .collect::<Vec<_>>();
      // prevent moving mid-rename
      snames.sort_by_key(|t| t.1);
      cells.main_wrap = false;
      ui.vertical_centered(|ui| {
        ui.strong("Solvers:");
        if ui.button("Add binary").clicked() {
          self.try_run(ui, Gui::add_solver_bin);
        }
        if ui.button("Add F06 directory").clicked() {
          self.try_run(ui, Gui::add_solver_dir);
        }
        TableBuilder::new(ui)
          .vscroll(true)
          .auto_shrink(true)
          .striped(true)
          .cell_layout(cells)
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .column(Column::auto().resizable(true))
          .header(heading_height, |mut header| {
            header.col(|ui| {
              ui.heading("Solver nickname");
            });
            header.col(|ui| {
              ui.heading("F06 acquisition method");
            });
            header.col(|ui| {
              ui.heading("Current reference solver");
            });
            header.col(|ui| {
              ui.heading("Current solver under test");
            });
            header.col(|ui| {
              ui.heading("Actions");
            });
          })
          .body(|body| {
            body.rows(body_height, nsolvers, |mut row| {
              let (_name, uuid) = snames.get(row.index()).unwrap();
              let solver = self
                .state
                .solvers
                .get_mut(uuid)
                .expect("missing solver UUID!");
              // nickname
              row.col(|ui| {
                ui.text_edit_singleline(&mut solver.nickname);
              });
              // method
              row.col(|ui| {
                ui.label(match &solver.method {
                  RunMethod::ImportFromDir(p) => {
                    format!("Get from {}", p.display())
                  }
                  RunMethod::RunSolver(p) => {
                    format!("Run solver {}", p.display())
                  }
                });
              });
              // columns for solver picks
              for pick in SolverPick::all() {
                let tgt = match pick {
                  SolverPick::Reference => &mut self.state.runner.ref_solver,
                  SolverPick::Testing => &mut self.state.runner.test_solver,
                };
                row.col(|ui| {
                  if *tgt == Some(*uuid) {
                    ui.label("Is current");
                  } else if ui.button("Make current").clicked() {
                    *tgt = Some(*uuid);
                  }
                });
              }
              // actions
              row.col(|ui| {
                if ui.button("Change path").clicked() {
                  self.change_solver(*uuid).ok();
                }
                if ui.button("Remove").clicked() {
                  self.state.remove_solver(*uuid);
                }
              });
            });
          });
      });
    });
  }

  /// Render function for a deck's results, side-by-side.
  fn view_results(&mut self, ctx: &Context) {
    // ensure deck
    let d = if let Some(u) = self.static_fields.current_deck {
      u
    } else {
      warn!("tried to view results with no deck!");
      return;
    };
    // block ref option to string
    let obref_str = |o: &Option<BlockRef>| -> String {
      if let Some(bref) = o {
        return format!("Subcase {}, {}", bref.subcase, bref.block_type);
      } else {
        return "Results summary".to_string();
      }
    };
    // show block in column
    let formatter = FloatFormat::default();
    let block_table =
      |ui: &mut Ui,
       block: &FinalBlock,
       oe: Option<&BTreeSet<DatumIndex>>,
       hf: Option<&BTreeSet<DatumIndex>>| {
        let heading_height = ui.text_style_height(&TextStyle::Heading);
        let dy = ui.spacing().item_spacing.y;
        let body_height = ui.text_style_height(&TextStyle::Body) + dy;
        let mut cells = Layout::left_to_right(Align::Center);
        cells.main_wrap = false;
        let rows: BTreeMap<usize, NasIndex> = block
          .row_indices
          .keys()
          .filter(|ri| {
            oe.is_none() || oe.is_some_and(|k| k.iter().any(|d| &&d.row == ri))
          })
          .copied()
          .enumerate()
          .collect();
        let cols: BTreeMap<usize, NasIndex> = block
          .col_indices
          .keys()
          .filter(|ci| {
            oe.is_none() || oe.is_some_and(|k| k.iter().any(|d| &&d.col == ci))
          })
          .copied()
          .enumerate()
          .collect();
        TableBuilder::new(ui)
          .vscroll(true)
          .auto_shrink(true)
          .striped(true)
          .cell_layout(cells)
          .columns(Column::auto(), cols.len() + 1)
          .header(heading_height, |mut header| {
            header.col(|ui| {
              ui.label("Row/Col");
            });
            for col_index in cols.values() {
              // column indexes heading
              header.col(|ui| {
                ui.strong(col_index.to_string());
              });
            }
          })
          .body(|body| {
            body.rows(body_height, rows.len(), |mut row| {
              let row_index = rows.get(&row.index()).unwrap();
              // row indexes column
              row.col(|ui| {
                ui.strong(row_index.to_string());
              });
              for col_index in block.col_indices.keys() {
                // data rows
                row.col(|ui| {
                  let mut fbuf = String::new();
                  let x = block.get(*row_index, *col_index).unwrap();
                  formatter.fmt_f64(&mut fbuf, x.into()).ok();
                  let mut rt =
                    RichText::new(fbuf).family(FontFamily::Monospace);
                  let di = DatumIndex {
                    block_ref: block.block_ref(),
                    row: *row_index,
                    col: *col_index,
                  };
                  if hf.is_some_and(|f| f.contains(&di)) {
                    rt = rt.color(Color32::RED);
                  }
                  ui.label(rt);
                });
              }
            });
          });
      };
    // column to show a block
    let block_col = |ui: &mut Ui,
                     rs: &RunState,
                     br: BlockRef,
                     oe: Option<&BTreeSet<DatumIndex>>,
                     hf: Option<&BTreeSet<DatumIndex>>| {
      if let RunState::Finished(f) = rs {
        if let Some(fb) = f.blocks.get(&br) {
          block_table(ui, &fb[0], oe, hf);
        } else {
          ui.label("Block absent!");
        }
      } else {
        ui.label("F06 absent!");
      }
    };
    // show per-extraction metrics
    let exn_metrics = |ui: &mut Ui, exr: &ExtractionResults| {
      let heading_height = ui.text_style_height(&TextStyle::Heading);
      let dy = ui.spacing().item_spacing.y;
      let body_height = ui.text_style_height(&TextStyle::Body) + dy;
      let cells = Layout::left_to_right(Align::Center);
      let formatter = FloatFormat::default();
      for pick in SolverPick::all() {
        match pick {
          SolverPick::Reference => ui.label("  => Reference solver:"),
          SolverPick::Testing => ui.label("  => Solver under test:"),
        };
        let metrics = SingleColumnMetric::all();
        let mut rows: Vec<(BlockRef, NasIndex)> = exr
          .col_metrics
          .keys()
          .filter_map(|scmi| {
            if scmi.0 == *pick {
              Some((scmi.1, scmi.2))
            } else {
              None
            }
          })
          .collect();
        rows.sort();
        rows.dedup();
        ui.push_id(pick, |ui| {
          TableBuilder::new(ui)
            .vscroll(false)
            .auto_shrink(true)
            .striped(true)
            .cell_layout(cells)
            .columns(Column::auto(), metrics.len() + 1)
            .header(heading_height, |mut header| {
              header.col(|ui| {
                ui.label("Col/Metric");
              });
              for metric in metrics {
                // column indexes heading
                header.col(|ui| {
                  ui.strong(metric.short_name());
                });
              }
            })
            .body(|body| {
              body.rows(body_height, rows.len(), |mut row| {
                let (bref, col) = rows.get(row.index()).unwrap();
                row.col(|ui| {
                  ui.strong(format!(
                    "Subcase {}, {}, {}",
                    bref.subcase, bref.block_type, col
                  ));
                });
                for scm in metrics {
                  let scmi = (*pick, *bref, *col, *scm);
                  let mut fbuf = String::new();
                  if let Some(Some(val)) = exr.col_metrics.get(&scmi) {
                    formatter.fmt_f64(&mut fbuf, *val).ok();
                  } else {
                    write!(&mut fbuf, "N/A").ok();
                  }
                  let rt = RichText::new(fbuf).family(FontFamily::Monospace);
                  row.col(|ui| {
                    ui.label(rt);
                  });
                }
              })
            });
        });
        ui.separator();
      }
      // column compare metrics
      ui.label("  => Column-compare metrics:");
      let mut rows: Vec<(BlockRef, NasIndex)> = exr
        .col_compares
        .keys()
        .map(|ccmi| (ccmi.0, ccmi.1))
        .collect();
      rows.sort();
      rows.dedup();
      let metrics = ColumnCompareMetric::all();
      TableBuilder::new(ui)
        .vscroll(false)
        .auto_shrink(true)
        .striped(true)
        .cell_layout(cells)
        .columns(Column::auto(), metrics.len() + 1)
        .header(heading_height, |mut header| {
          header.col(|ui| {
            ui.label("Col/Metric");
          });
          for metric in metrics {
            // column indexes heading
            header.col(|ui| {
              ui.strong(metric.short_name());
            });
          }
        })
        .body(|body| {
          body.rows(body_height, rows.len(), |mut row| {
            let (bref, col) = rows.get(row.index()).unwrap();
            row.col(|ui| {
              ui.strong(format!(
                "Subcase {}, {}, {}",
                bref.subcase, bref.block_type, col
              ));
            });
            for ccm in metrics {
              let ccmi = (*bref, *col, *ccm);
              let mut fbuf = String::new();
              if let Some(Some(val)) = exr.col_compares.get(&ccmi) {
                formatter.fmt_f64(&mut fbuf, *val).ok();
              } else {
                write!(&mut fbuf, "N/A").ok();
              }
              let rt = RichText::new(fbuf).family(FontFamily::Monospace);
              row.col(|ui| {
                ui.label(rt);
              });
            }
          })
        });
      ui.separator();
    };
    egui::CentralPanel::default().show(ctx, |ui| {
      self.show_menu(ctx, ui);
      let (deck, res_mtx) = self.state.get_deck(d).expect("bad deck uuid");
      let deck_name = deck.name().to_owned();
      let res = res_mtx.lock().expect("results mutex poisoned");
      let sf = &mut self.static_fields;
      let deck_data = self.state.decks_names().collect::<Vec<(_, _)>>();
      ui.horizontal(|ui| {
        // deck picker
        ComboBox::from_id_source("res_deck_picker")
          .selected_text(deck_name)
          .show_ui(ui, |ui| {
            for (s, u) in deck_data {
              ui.selectable_value(&mut sf.current_deck, Some(u), s);
            }
          });
        // blockref picker
        let bref = &mut sf.block_ref;
        ComboBox::from_id_source("res_block_picker")
          .selected_text(obref_str(bref))
          .show_ui(ui, |ui| {
            ui.selectable_value(bref, None, obref_str(&None));
            for cand in res.all_block_refs() {
              ui.selectable_value(bref, Some(cand), obref_str(&Some(cand)));
            }
          });
        // limit to extracted
        ui.checkbox(
          &mut sf.extractions_only,
          "Show only values in extractions",
        );
        // highlight flagged
        ui.checkbox(&mut sf.highlight_flagged, "Highlight flagged values");
      });
      if let Some(bref) = sf.block_ref {
        // show chosen block
        ui.columns(2, |cols| {
          for (i, pick) in SolverPick::all().iter().enumerate() {
            cols[i].vertical_centered(|ui| {
              ui.push_id(i, |ui| {
                block_col(
                  ui,
                  res.get(*pick),
                  bref,
                  if sf.extractions_only {
                    Some(&res.extracted)
                  } else {
                    None
                  },
                  if sf.highlight_flagged {
                    Some(&res.flagged)
                  } else {
                    None
                  },
                );
              })
            });
          }
        });
      } else {
        // show metrics
        for (exno, exr) in res.extractions.iter().enumerate() {
          ui.strong(format!("==> For extraction #{exno}:"));
          ScrollArea::vertical().show(ui, |ui| {
            exn_metrics(ui, exr);
          });
        }
      }
    });
  }
}

impl eframe::App for Gui {
  fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
    //if cfg!(debug_assertions) {
    //  ctx.set_debug_on_hover(true);
    //}
    match self.view {
      View::Decks => self.view_decks(ctx),
      View::Solvers => self.view_solvers(ctx),
      View::CriteriaSets => self.view_criteria_sets(ctx),
      View::Extractions => self.view_deck_exns(ctx),
      View::Results => self.view_results(ctx),
    };
  }
}
