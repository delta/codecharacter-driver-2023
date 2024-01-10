use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub enum GameStatusEnum {
    IDLE,
    EXECUTING,
    EXECUTED,
    EXECUTE_ERROR,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct GameResult {
    pub destruction_percentage: f64,
    pub coins_used: u64,
    pub has_errors: bool,
    pub log: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct GameStatus {
    pub game_id: String,
    pub game_status: GameStatusEnum,
    pub game_result: Option<GameResult>,
    pub game_result_player1: Option<GameResult>,
    pub game_result_player2: Option<GameResult>,
}

impl GameStatus {
    pub fn new_normal(
        game_id: String,
        game_status: GameStatusEnum,
        game_result: Option<GameResult>,
    ) -> Self {
        GameStatus {
            game_id,
            game_status,
            game_result,
            game_result_player1: None,
            game_result_player2: None,
        }
    }

    pub fn new_pvp(
        game_id: String,
        game_status: GameStatusEnum,
        game_result_player1: Option<GameResult>,
        game_result_player2: Option<GameResult>,
    ) -> Self {
        GameStatus {
            game_id,
            game_status,
            game_result: None,
            game_result_player1,
            game_result_player2,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::{GameStatus, GameStatusEnum};
    #[test]
    pub fn serialization_test() {
        // An example respone
        let expected_response = r#"{"game_id":"030af985-f4b5-4914-94d8-e559576449e3","game_status":"EXECUTING","game_result":null,"game_result_player1":null,"game_result_player2":null}"#;

        let game_status = GameStatus::new_normal(
            "030af985-f4b5-4914-94d8-e559576449e3".to_string(),
            GameStatusEnum::EXECUTING,
            None,
        );

        let serialized_game_status = serde_json::to_string(&game_status).unwrap();

        assert_eq!(serialized_game_status, expected_response);
    }
}
