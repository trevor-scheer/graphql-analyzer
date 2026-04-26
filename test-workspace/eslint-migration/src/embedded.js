import { gql } from "@apollo/client";

const GET_USER = gql`
  query {
    user(id: "1") { name }
  }
`;
