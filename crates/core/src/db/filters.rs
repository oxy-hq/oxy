use entity::users::{self, UserStatus};
use sea_orm::{ColumnTrait, Condition, QueryFilter, Select};

pub struct UserFilters;

impl UserFilters {
    pub fn active() -> Condition {
        Condition::all().add(users::Column::Status.eq(UserStatus::Active))
    }
    pub fn by_email(email: &str) -> Condition {
        Condition::all().add(users::Column::Email.eq(email))
    }
    pub fn active_by_email(email: &str) -> Condition {
        Condition::all()
            .add(users::Column::Status.eq(UserStatus::Active))
            .add(users::Column::Email.eq(email))
    }

    pub fn by_verification_token(token: &str) -> Condition {
        Condition::all().add(users::Column::EmailVerificationToken.eq(token))
    }

    pub fn active_by_verification_token(token: &str) -> Condition {
        Condition::all()
            .add(users::Column::Status.eq(UserStatus::Active))
            .add(users::Column::EmailVerificationToken.eq(token))
    }
}

pub trait UserQueryFilterExt<E>
where
    E: sea_orm::EntityTrait,
{
    fn filter_active(self) -> Select<E>;

    fn filter_by_email(self, email: &str) -> Select<E>;

    fn filter_active_by_email(self, email: &str) -> Select<E>;

    fn filter_active_by_verification_token(self, token: &str) -> Select<E>;
}

impl UserQueryFilterExt<entity::users::Entity> for Select<entity::users::Entity> {
    fn filter_active(self) -> Select<entity::users::Entity> {
        self.filter(UserFilters::active())
    }

    fn filter_by_email(self, email: &str) -> Select<entity::users::Entity> {
        self.filter(UserFilters::by_email(email))
    }

    fn filter_active_by_email(self, email: &str) -> Select<entity::users::Entity> {
        self.filter(UserFilters::active_by_email(email))
    }

    fn filter_active_by_verification_token(self, token: &str) -> Select<entity::users::Entity> {
        self.filter(UserFilters::active_by_verification_token(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::users::{self, UserStatus};
    use sea_orm::{ColumnTrait, Condition};

    #[test]
    fn test_active_filter() {
        let condition = UserFilters::active();
        let expected = Condition::all().add(users::Column::Status.eq(UserStatus::Active));

        assert_eq!(format!("{condition:?}"), format!("{:?}", expected));
    }

    #[test]
    fn test_by_email_filter() {
        let email = "test@example.com";
        let condition = UserFilters::by_email(email);
        let expected = Condition::all().add(users::Column::Email.eq(email));

        assert_eq!(format!("{condition:?}"), format!("{:?}", expected));
    }

    #[test]
    fn test_active_by_email_filter() {
        let email = "test@example.com";
        let condition = UserFilters::active_by_email(email);
        let expected = Condition::all()
            .add(users::Column::Status.eq(UserStatus::Active))
            .add(users::Column::Email.eq(email));

        assert_eq!(format!("{condition:?}"), format!("{:?}", expected));
    }

    #[test]
    fn test_by_verification_token_filter() {
        let token = "test-token-123";
        let condition = UserFilters::by_verification_token(token);
        let expected = Condition::all().add(users::Column::EmailVerificationToken.eq(token));

        assert_eq!(format!("{condition:?}"), format!("{:?}", expected));
    }

    #[test]
    fn test_active_by_verification_token_filter() {
        let token = "test-token-123";
        let condition = UserFilters::active_by_verification_token(token);
        let expected = Condition::all()
            .add(users::Column::Status.eq(UserStatus::Active))
            .add(users::Column::EmailVerificationToken.eq(token));

        assert_eq!(format!("{condition:?}"), format!("{:?}", expected));
    }

    #[test]
    fn test_email_filter_with_different_emails() {
        let email1 = "user1@example.com";
        let email2 = "user2@example.com";

        let condition1 = UserFilters::by_email(email1);
        let condition2 = UserFilters::by_email(email2);

        assert_ne!(format!("{condition1:?}"), format!("{:?}", condition2));
    }

    #[test]
    fn test_active_vs_non_active_filters() {
        let email = "test@example.com";

        let active_condition = UserFilters::active_by_email(email);
        let email_only_condition = UserFilters::by_email(email);

        assert_ne!(
            format!("{active_condition:?}"),
            format!("{:?}", email_only_condition)
        );
    }
}
